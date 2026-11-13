// =============================================================================
// G5 Workspace — review drafts
// =============================================================================
// What a reviewer writes about their own changes before pushing them: a note
// per change (the owner reads it as the suggestion's opening message) and
// which changes or files to leave out of the next round. Drafts live in this
// browser only, per review workspace, until Push review sends them; nothing
// here is shared or read by the server before that.
//
// One store per review workspace: every mounted consumer (the My review
// panel, the Push review dialog) reads the same snapshot and hears every
// write or clear, whichever of them made it, and a change made in another
// tab reaches them through the browser's storage event. When the browser
// refuses to store (a full quota, a private window), the memory copy is the
// truth until a later write lands.
// =============================================================================

import { useCallback, useSyncExternalStore } from "react";
import type { ExcludedReviewFile, ReviewPreview } from "../../lib/types";

export interface ReviewDrafts {
  /** Note per change key (the preview's `changes[].key`). */
  changeNotes: Record<string, string>;
  /** Change keys left out of the next push. */
  excludedChanges: string[];
  /** Note per path (new files and whole-file notes). */
  fileNotes: Record<string, string>;
  /** Files (one repository's path each) left out of the next push. */
  excludedFiles: ExcludedReviewFile[];
}

export const EMPTY_REVIEW_DRAFTS: ReviewDrafts = {
  changeNotes: {},
  excludedChanges: [],
  fileNotes: {},
  excludedFiles: [],
};

/** The identity of one file of one repository, as the drafts hold it. */
export function reviewFileKey(repoId: string, path: string): string {
  return `${repoId}\u0000${path}`;
}

/** Whether the drafts leave `path` of `repoId` out of the next push. */
export function isReviewFileExcluded(
  drafts: ReviewDrafts,
  repoId: string,
  path: string,
): boolean {
  return drafts.excludedFiles.some(
    (file) => file.repoId === repoId && file.path === path,
  );
}

function storageKey(reviewWorkspaceId: string): string {
  return `g5.review-drafts:${reviewWorkspaceId}`;
}

function stringRecord(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object") return {};
  const out: Record<string, string> = {};
  for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
    if (typeof entry === "string") out[key] = entry;
  }
  return out;
}

function stringList(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((entry): entry is string => typeof entry === "string")
    : [];
}

function fileList(value: unknown): ExcludedReviewFile[] {
  if (!Array.isArray(value)) return [];
  const out: ExcludedReviewFile[] = [];
  for (const entry of value) {
    if (!entry || typeof entry !== "object") continue;
    const { repoId, path } = entry as Record<string, unknown>;
    if (typeof repoId === "string" && typeof path === "string") {
      out.push({ repoId, path });
    }
  }
  return out;
}

function parseDrafts(raw: string | null): ReviewDrafts {
  if (!raw) return EMPTY_REVIEW_DRAFTS;
  try {
    const parsed = JSON.parse(raw) as Partial<ReviewDrafts>;
    return {
      changeNotes: stringRecord(parsed.changeNotes),
      excludedChanges: stringList(parsed.excludedChanges),
      fileNotes: stringRecord(parsed.fileNotes),
      excludedFiles: fileList(parsed.excludedFiles),
    };
  } catch {
    return EMPTY_REVIEW_DRAFTS;
  }
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

type Listener = () => void;

/** Listeners per review workspace id. */
const listeners = new Map<string, Set<Listener>>();

/**
 * The last snapshot handed out per review workspace, with the raw storage
 * string it was parsed from: a render gets the same object back while the
 * storage is unchanged (what `useSyncExternalStore` needs), and a new one
 * once anything wrote to it, here or in another tab. `raw === undefined`
 * marks a memory-only snapshot (storage unreadable, or a write the browser
 * refused): it is the truth until a later write or clear lands, whatever
 * the storage still says.
 */
const snapshots = new Map<
  string,
  { raw: string | null | undefined; drafts: ReviewDrafts }
>();

function notify(reviewWorkspaceId: string): void {
  for (const listener of listeners.get(reviewWorkspaceId) ?? []) listener();
}

function snapshot(reviewWorkspaceId: string): ReviewDrafts {
  if (!reviewWorkspaceId) return EMPTY_REVIEW_DRAFTS;
  const cached = snapshots.get(reviewWorkspaceId);
  if (cached && cached.raw === undefined) return cached.drafts;
  let raw: string | null | undefined;
  try {
    raw = localStorage.getItem(storageKey(reviewWorkspaceId));
  } catch {
    raw = undefined;
  }
  if (raw === undefined) return cached?.drafts ?? EMPTY_REVIEW_DRAFTS;
  if (cached && cached.raw === raw) return cached.drafts;
  const drafts = parseDrafts(raw);
  snapshots.set(reviewWorkspaceId, { raw, drafts });
  return drafts;
}

/** The drafts of `reviewWorkspaceId` as the store sees them (empty when none). */
export function readReviewDrafts(reviewWorkspaceId: string): ReviewDrafts {
  return snapshot(reviewWorkspaceId);
}

function subscribe(reviewWorkspaceId: string, listener: Listener): () => void {
  if (!reviewWorkspaceId) return () => {};
  let set = listeners.get(reviewWorkspaceId);
  if (!set) {
    set = new Set();
    listeners.set(reviewWorkspaceId, set);
  }
  set.add(listener);
  // Another tab of this browser wrote or cleared the same drafts.
  const onStorage = (event: StorageEvent) => {
    if (event.key === null || event.key === storageKey(reviewWorkspaceId)) {
      listener();
    }
  };
  if (typeof window !== "undefined") {
    window.addEventListener("storage", onStorage);
  }
  return () => {
    set?.delete(listener);
    if (typeof window !== "undefined") {
      window.removeEventListener("storage", onStorage);
    }
  };
}

export function writeReviewDrafts(
  reviewWorkspaceId: string,
  drafts: ReviewDrafts,
): void {
  if (!reviewWorkspaceId) return;
  const raw = JSON.stringify(drafts);
  let stored: string | undefined = raw;
  try {
    localStorage.setItem(storageKey(reviewWorkspaceId), raw);
  } catch {
    // Storage refused (private window, quota): the memory copy is the truth
    // until a later write lands.
    stored = undefined;
  }
  snapshots.set(reviewWorkspaceId, { raw: stored, drafts });
  notify(reviewWorkspaceId);
}

export function clearReviewDrafts(reviewWorkspaceId: string): void {
  if (!reviewWorkspaceId) return;
  let stored: string | null | undefined = null;
  try {
    localStorage.removeItem(storageKey(reviewWorkspaceId));
  } catch {
    // Nothing to clear where nothing could be stored.
    stored = undefined;
  }
  snapshots.set(reviewWorkspaceId, {
    raw: stored,
    drafts: EMPTY_REVIEW_DRAFTS,
  });
  notify(reviewWorkspaceId);
}

/**
 * Drafts without empty file notes and without duplicate holds. An empty
 * change note is kept: it is the reviewer clearing the note the round
 * carries, which the push must be told ("" replaces it; no entry keeps it).
 * `pruneReviewDrafts` drops it only with the key the preview no longer lists.
 */
function tidy(drafts: ReviewDrafts): ReviewDrafts {
  const changeNotes: Record<string, string> = {};
  for (const [key, note] of Object.entries(drafts.changeNotes)) {
    changeNotes[key] = note.trim() ? note : "";
  }
  const fileNotes: Record<string, string> = {};
  for (const [path, note] of Object.entries(drafts.fileNotes)) {
    if (note.trim()) fileNotes[path] = note;
  }
  const seenFiles = new Set<string>();
  const excludedFiles: ExcludedReviewFile[] = [];
  for (const file of drafts.excludedFiles) {
    const key = reviewFileKey(file.repoId, file.path);
    if (seenFiles.has(key)) continue;
    seenFiles.add(key);
    excludedFiles.push({ repoId: file.repoId, path: file.path });
  }
  return {
    changeNotes,
    excludedChanges: [...new Set(drafts.excludedChanges)],
    fileNotes,
    excludedFiles,
  };
}

function sameDrafts(a: ReviewDrafts, b: ReviewDrafts): boolean {
  return JSON.stringify(tidy(a)) === JSON.stringify(tidy(b));
}

/**
 * The drafts that still name something the preview lists. A note or a hold
 * on a change the reviewer edited since (its key changed) or on a file no
 * longer in the round is stale: the server refuses a push that carries a
 * key it cannot find, and nothing on the panel could clear it.
 */
export function pruneReviewDrafts(
  drafts: ReviewDrafts,
  preview: ReviewPreview,
): ReviewDrafts {
  const keys = new Set<string>();
  const paths = new Set<string>();
  const files = new Set<string>();
  for (const file of preview.files) {
    paths.add(file.path);
    files.add(reviewFileKey(file.repoId, file.path));
    for (const change of file.changes ?? []) keys.add(change.key);
  }
  // An empty note on a change the preview still lists is kept: it clears
  // the note a push would otherwise carry over (across the owner's commits
  // too, when the preview no longer says "in round N"). Only a key the
  // preview dropped is forgotten.
  const changeNotes: Record<string, string> = {};
  for (const [key, note] of Object.entries(drafts.changeNotes)) {
    if (keys.has(key)) changeNotes[key] = note;
  }
  const fileNotes: Record<string, string> = {};
  for (const [path, note] of Object.entries(drafts.fileNotes)) {
    if (paths.has(path)) fileNotes[path] = note;
  }
  return tidy({
    changeNotes,
    excludedChanges: drafts.excludedChanges.filter((key) => keys.has(key)),
    fileNotes,
    excludedFiles: drafts.excludedFiles.filter((file) =>
      files.has(reviewFileKey(file.repoId, file.path)),
    ),
  });
}

export interface ReviewDraftsHandle {
  drafts: ReviewDrafts;
  setChangeNote: (key: string, note: string) => void;
  setChangeIncluded: (key: string, included: boolean) => void;
  setFileNote: (path: string, note: string) => void;
  setFileIncluded: (repoId: string, path: string, included: boolean) => void;
  /** Drop what the latest preview no longer lists (see `pruneReviewDrafts`). */
  reconcile: (preview: ReviewPreview) => void;
  clear: () => void;
}

/**
 * The drafts of one review workspace: one shared store per id, persisted on
 * every change, the same snapshot for every mounted consumer.
 */
export function useReviewDrafts(reviewWorkspaceId: string): ReviewDraftsHandle {
  const drafts = useSyncExternalStore(
    useCallback(
      (listener: Listener) => subscribe(reviewWorkspaceId, listener),
      [reviewWorkspaceId],
    ),
    () => snapshot(reviewWorkspaceId),
    () => snapshot(reviewWorkspaceId),
  );
  const update = useCallback(
    (change: (current: ReviewDrafts) => ReviewDrafts) => {
      const next = tidy(change(snapshot(reviewWorkspaceId)));
      writeReviewDrafts(reviewWorkspaceId, next);
    },
    [reviewWorkspaceId],
  );
  const setChangeNote = useCallback(
    (key: string, note: string) =>
      update((current) => ({
        ...current,
        changeNotes: { ...current.changeNotes, [key]: note },
      })),
    [update],
  );
  const setChangeIncluded = useCallback(
    (key: string, included: boolean) =>
      update((current) => ({
        ...current,
        excludedChanges: included
          ? current.excludedChanges.filter((entry) => entry !== key)
          : [...current.excludedChanges, key],
      })),
    [update],
  );
  const setFileNote = useCallback(
    (path: string, note: string) =>
      update((current) => ({
        ...current,
        fileNotes: { ...current.fileNotes, [path]: note },
      })),
    [update],
  );
  const setFileIncluded = useCallback(
    (repoId: string, path: string, included: boolean) =>
      update((current) => ({
        ...current,
        excludedFiles: included
          ? current.excludedFiles.filter(
              (file) => !(file.repoId === repoId && file.path === path),
            )
          : [...current.excludedFiles, { repoId, path }],
      })),
    [update],
  );
  const reconcile = useCallback(
    (preview: ReviewPreview) => {
      const current = snapshot(reviewWorkspaceId);
      const pruned = pruneReviewDrafts(current, preview);
      if (!sameDrafts(current, pruned)) {
        writeReviewDrafts(reviewWorkspaceId, pruned);
      }
    },
    [reviewWorkspaceId],
  );
  const clear = useCallback(() => {
    clearReviewDrafts(reviewWorkspaceId);
  }, [reviewWorkspaceId]);
  return {
    drafts,
    setChangeNote,
    setChangeIncluded,
    setFileNote,
    setFileIncluded,
    reconcile,
    clear,
  };
}

/**
 * Whether the drafts hold anything at all. An explicit empty change note
 * counts: it is a pending deletion the push sends, and Clear all drafts is
 * how it is discarded.
 */
export function hasReviewDrafts(drafts: ReviewDrafts): boolean {
  const clean = tidy(drafts);
  return (
    Object.keys(clean.changeNotes).length > 0 ||
    clean.excludedChanges.length > 0 ||
    Object.keys(clean.fileNotes).length > 0 ||
    clean.excludedFiles.length > 0
  );
}

/** How many of the previewed changes the drafts keep in, and how many carry a note. */
export function draftSummary(
  preview: ReviewPreview,
  drafts: ReviewDrafts,
): { included: number; total: number; notes: number } {
  let included = 0;
  let total = 0;
  let notes = 0;
  const excludedChanges = new Set(drafts.excludedChanges);
  for (const file of preview.files) {
    const fileExcluded = isReviewFileExcluded(drafts, file.repoId, file.path);
    for (const change of file.changes ?? []) {
      const edits = change.editCount ?? 1;
      total += edits;
      if (!fileExcluded && !excludedChanges.has(change.key)) {
        included += edits;
      }
      if (drafts.changeNotes[change.key]?.trim()) notes += 1;
    }
  }
  return { included, total, notes };
}

/**
 * The body fields Push review sends from the drafts: only what the preview
 * it is pushing from still lists, so a stale key never reaches the server.
 */
export function draftPublishFields(
  drafts: ReviewDrafts,
  preview?: ReviewPreview,
): {
  changeNotes?: Record<string, string>;
  excludedChanges?: string[];
  notes?: Record<string, string>;
  excludedFiles?: ExcludedReviewFile[];
} {
  const clean = preview ? pruneReviewDrafts(drafts, preview) : tidy(drafts);
  return {
    changeNotes: Object.keys(clean.changeNotes).length
      ? clean.changeNotes
      : undefined,
    excludedChanges: clean.excludedChanges.length
      ? clean.excludedChanges
      : undefined,
    notes: Object.keys(clean.fileNotes).length ? clean.fileNotes : undefined,
    excludedFiles: clean.excludedFiles.length ? clean.excludedFiles : undefined,
  };
}
