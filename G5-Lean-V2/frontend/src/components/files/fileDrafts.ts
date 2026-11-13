// =============================================================================
// G5 Files — fileDrafts
// =============================================================================
// Unsaved FILES edits, held for the browser tab rather than by an editor
// view (`g5.documents.files-view.drafts`). The FILES view, its editor and
// the workspace page unmount on tab, panel, HTML-mode and page switches; a
// draft here survives all of them and is restored when its file is shown
// again. Keys carry the signed-in person: another person never sees a draft
// (`g5.security.login.principal`), and their drafts are dropped as soon as
// the person changes. An org switch keeps the person's drafts; workspace
// ids keep each org's drafts on that org's pages.
//
// Closing or reloading the tab is the only way left to lose a draft, so the
// store asks the browser to confirm leaving while any draft exists.
// =============================================================================

import { useSyncExternalStore } from "react";
import type { editor } from "monaco-editor";
import { ApiError } from "../../lib/api";

/** The file version a draft started from, which Save or Push compares with. */
export interface FileDraftBaseline {
  content: string;
  /** Git blob hash of `content` as read; null for a file not read yet. */
  hash: string | null;
  /** The VM that supplied the bytes, for an owner's live read. */
  vmId: string | null;
}

/** Whether Save writes the draft (owner) or Push review suggests it (reader). */
export type FileDraftRole = "owner" | "suggest";

export interface FileDraft {
  /**
   * Changes whenever a new draft starts, so a late completion of a Save or
   * Push that began on an older draft can never change or recreate this one.
   */
  id: string;
  role: FileDraftRole;
  content: string;
  baseline: FileDraftBaseline;
  /**
   * A Push review of this content is in flight: every view of the draft
   * takes no typing until it settles, so what was pushed is all there is.
   */
  pushing?: boolean;
  /**
   * Saves of this draft still in flight. While any is, the draft stays even
   * when its text equals its baseline: the file is about to change under it,
   * so text typed back to the old version is a later edit.
   */
  saving?: number;
  /** Failure follows the draft through navigation. */
  saveError?: string;
  saveConflict?: { currentHash: string; currentContent?: string };
  /**
   * The Save that settled last: the version it replaced (the hash it
   * expected) and the one it wrote (null when refused). Every view drops a
   * version that Save superseded instead of showing it as a newer one.
   */
  lastSave?: { over: string | null; saved: string | null };
}

export interface FileDraftAddress {
  /** The signed-in person (a principal's person, never its org). */
  person: string;
  workspaceId: string;
  repoId: string;
  path: string;
}

export interface WorkspaceFileDraft {
  key: string;
  repoId: string;
  path: string;
  draft: FileDraft;
}

type Listener = () => void;

const drafts = new Map<string, FileDraft>();
// Models, the selected mode and whether the person was editing share the
// person/file lifetime, including when clean: Save and switching modes must
// not erase native undo. No second source buffer.
/**
 * The rendered document ("preview": the reading view, or Rich text while
 * editing Markdown) or plain text ("source") (`g5.documents.editor.modes`).
 */
export type FileViewMode = "source" | "preview";

const sessions = new Map<
  string,
  {
    model?: editor.ITextModel;
    mode?: FileViewMode;
    editing?: boolean;
    viewState?: editor.ICodeEditorViewState | null;
  }
>();

/** The selected mode survives navigation even before a source model is loaded. */
export function fileViewMode(key: string): FileViewMode | undefined {
  return sessions.get(key)?.mode;
}

export function saveFileViewMode(key: string, mode: FileViewMode): void {
  const session = sessions.get(key);
  if (session) session.mode = mode;
  else sessions.set(key, { mode });
}

/**
 * Whether the person was editing when they left: a file reopens editing, or
 * reading with its draft applied after Done (`g5.documents.editor.modes`).
 */
export function fileEditing(key: string): boolean | undefined {
  return sessions.get(key)?.editing;
}

export function saveFileEditing(key: string, editing: boolean): void {
  const session = sessions.get(key);
  if (session) session.editing = editing;
  else sessions.set(key, { editing });
}

export function fileModelUri(key: string): string {
  return `g5-file://session/${encodeURIComponent(key)}`;
}

export function retainFileModel(key: string, model: editor.ITextModel): void {
  const previous = sessions.get(key);
  if (previous?.model === model) return;
  previous?.model?.dispose();
  sessions.set(key, { ...previous, model });
}

export function fileModelViewState(
  key: string,
): editor.ICodeEditorViewState | null {
  return sessions.get(key)?.viewState ?? null;
}

export function saveFileModelViewState(
  key: string,
  state: editor.ICodeEditorViewState | null,
): void {
  const session = sessions.get(key);
  if (session) session.viewState = state;
}

/** Discard is explicit; unlike Save it starts a fresh native undo history. */
export function resetFileModel(key: string, content: string): void {
  const session = sessions.get(key);
  if (session?.model && !session.model.isDisposed())
    session.model.setValue(content);
}

const listeners = new Set<Listener>();
let version = 0;
let draftSequence = 0;

/**
 * The person a principal key (`person:org`, `tokenPrincipal`) names: drafts
 * belong to people, so an org switch keeps them.
 */
export function draftPerson(principal: string | null): string | null {
  return principal ? principal.split(":")[0] || null : null;
}

/** One key per person, workspace, repository and path; unambiguous. */
export function fileDraftKey(address: FileDraftAddress): string {
  return JSON.stringify([
    address.person,
    address.workspaceId,
    address.repoId,
    address.path,
  ]);
}

function parseKey(key: string): FileDraftAddress | null {
  try {
    const parts: unknown = JSON.parse(key);
    if (
      Array.isArray(parts) &&
      parts.length === 4 &&
      parts.every((part) => typeof part === "string")
    ) {
      const [person, workspaceId, repoId, path] = parts as string[];
      return { person, workspaceId, repoId, path };
    }
  } catch {
    // Not a draft key.
  }
  return null;
}

/** A fresh draft identity. */
export function newFileDraftId(): string {
  draftSequence += 1;
  return `${Date.now().toString(36)}-${draftSequence}`;
}

function confirmLeaving(event: BeforeUnloadEvent): void {
  event.preventDefault();
  // Older browsers show the prompt only when returnValue is set.
  event.returnValue = "";
}

let unloadGuarded = false;

function syncUnloadGuard(): void {
  if (typeof window === "undefined") return;
  if (hasFileDrafts() && !unloadGuarded) {
    window.addEventListener("beforeunload", confirmLeaving);
    unloadGuarded = true;
  } else if (!hasFileDrafts() && unloadGuarded) {
    window.removeEventListener("beforeunload", confirmLeaving);
    unloadGuarded = false;
  }
}

function emit(): void {
  version += 1;
  syncUnloadGuard();
  for (const listener of listeners) listener();
}

export function getFileDraft(key: string): FileDraft | null {
  return drafts.get(key) ?? null;
}

/** Start, replace or (with null) drop the draft at `key`. */
export function setFileDraft(key: string, draft: FileDraft | null): void {
  if (draft) {
    if (drafts.get(key) === draft) return;
    drafts.set(key, draft);
  } else if (!drafts.delete(key)) {
    return;
  }
  emit();
}

/**
 * Change the draft at `key` only while it is still the draft `id`: a Save or
 * Push completing after the draft was discarded, or replaced by a newer one,
 * changes nothing. Returns whether the draft was found.
 */
export function updateFileDraft(
  key: string,
  id: string,
  change: (draft: FileDraft) => FileDraft | null,
): boolean {
  const current = drafts.get(key);
  if (!current || current.id !== id) return false;
  setFileDraft(key, change(current));
  return true;
}

/** One constructor for source edits and explicit preview rewrites. */
export function createFileDraft(
  role: FileDraftRole,
  content: string,
  baseline: FileDraftBaseline,
  id = newFileDraftId(),
): FileDraft {
  return { id, role, content, baseline };
}

export function fileVersionConflict(error: unknown): FileDraft["saveConflict"] {
  return error instanceof ApiError &&
    error.is("FILE_VERSION_CONFLICT") &&
    typeof error.details.hash === "string"
    ? {
        currentHash: error.details.hash,
        currentContent:
          typeof error.details.content === "string"
            ? error.details.content
            : undefined,
      }
    : undefined;
}

/** Guard explicit Source Save and preview writes against competing sessions. */
export function writeFileDraft(
  key: string,
  initial: FileDraft,
  {
    expectedContent,
    reserveSave = false,
  }: { expectedContent?: string; reserveSave?: boolean } = {},
): FileDraft | null {
  const current = getFileDraft(key);
  if (
    current &&
    (current.id !== initial.id ||
      current.role !== initial.role ||
      current.saving ||
      current.pushing)
  )
    return null;
  if (
    expectedContent !== undefined &&
    (current?.content ?? initial.baseline.content) !== expectedContent
  )
    return null;
  const next = {
    ...(current ?? initial),
    content: initial.content,
    ...(reserveSave
      ? { saving: 1, saveError: undefined, saveConflict: undefined }
      : {}),
  };
  setFileDraft(key, next);
  return next;
}

/** Reserve the one Save slot for this file. Every source and domain writer uses it. */
export function beginFileSave(
  key: string,
  initial: FileDraft,
  expectedHash: string | null,
) {
  // Publish the submitted buffer and its reservation atomically: subscribers
  // must never observe an unreserved Save between two store notifications.
  if (!writeFileDraft(key, initial, { reserveSave: true })) return null;
  let settled = false;
  return (
    baseline: FileDraftBaseline | null,
    error?: string,
    conflict?: FileDraft["saveConflict"],
    forgetIfClean = false,
  ) => {
    if (settled) return;
    settled = true;
    updateFileDraft(key, initial.id, (current) => {
      const next = {
        ...current,
        saving: undefined,
        baseline: baseline ?? current.baseline,
        saveError: error,
        saveConflict: conflict,
        lastSave: { over: expectedHash, saved: baseline?.hash ?? null },
      };
      return forgetIfClean && next.content === next.baseline.content
        ? null
        : next;
    });
  };
}

/** Drop every draft that does not belong to `person` (all for null). */
export function retainFileDraftsFor(person: string | null): void {
  let changed = false;
  for (const [key, session] of sessions) {
    if (person === null || parseKey(key)?.person !== person) {
      session.model?.dispose();
      sessions.delete(key);
    }
  }
  for (const key of Array.from(drafts.keys())) {
    if (person === null || parseKey(key)?.person !== person) {
      drafts.delete(key);
      changed = true;
    }
  }
  if (changed) emit();
}

export function subscribeFileDrafts(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The draft at `key`, as a live value; null without a key or draft. */
export function useFileDraft(key: string | null): FileDraft | null {
  return useSyncExternalStore(
    subscribeFileDrafts,
    () => (key ? getFileDraft(key) : null),
    () => null,
  );
}

const workspaceSnapshots = new Map<
  string,
  { version: number; entries: readonly WorkspaceFileDraft[] }
>();

/** Every draft of one person in one workspace. */
export function workspaceFileDrafts(
  person: string,
  workspaceId: string,
): readonly WorkspaceFileDraft[] {
  const cacheKey = JSON.stringify([person, workspaceId]);
  const cached = workspaceSnapshots.get(cacheKey);
  if (cached && cached.version === version) return cached.entries;
  const entries: WorkspaceFileDraft[] = [];
  for (const [key, draft] of drafts) {
    if (
      draft.content === draft.baseline.content &&
      !draft.saving &&
      !draft.pushing
    )
      continue;
    const address = parseKey(key);
    if (
      address &&
      address.person === person &&
      address.workspaceId === workspaceId
    ) {
      entries.push({ key, repoId: address.repoId, path: address.path, draft });
    }
  }
  workspaceSnapshots.set(cacheKey, { version, entries });
  return entries;
}

const NO_DRAFTS: readonly WorkspaceFileDraft[] = [];

/** `workspaceFileDrafts` as a live value. */
export function useWorkspaceFileDrafts(
  person: string | null,
  workspaceId: string | null,
): readonly WorkspaceFileDraft[] {
  return useSyncExternalStore(
    subscribeFileDrafts,
    () =>
      person && workspaceId
        ? workspaceFileDrafts(person, workspaceId)
        : NO_DRAFTS,
    () => NO_DRAFTS,
  );
}

/** Whether any draft exists (the tab would ask before closing). */
export function hasFileDrafts(): boolean {
  return Array.from(drafts.values()).some(
    (draft) =>
      draft.content !== draft.baseline.content ||
      !!draft.saving ||
      !!draft.pushing,
  );
}

/** Tests only: forget every draft and listener registration state. */
export function resetFileDraftsForTests(): void {
  drafts.clear();
  for (const session of sessions.values()) session.model?.dispose();
  sessions.clear();
  workspaceSnapshots.clear();
  emit();
}
