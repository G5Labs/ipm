// =============================================================================
// G5 Client — Recently-Opened Workspace MRU
// =============================================================================
// Per-browser, per-user localStorage MRU of workspace opens, feeding
// organization Home.
// "Recently opened — other people's workspaces" table. Entries carry ids only —
// names and ownership are always joined live against the fetched workspace
// lists so nothing in storage can go stale. Chosen over backend per-user
// open-history persistence by explicit user decision (2026-08-11); follows the
// explainer-template MRU idiom (pure list update + tolerant read + non-fatal
// write, shared through `storedList.ts`). Spec: software_architecture/client/routing-and-auth D24.
// =============================================================================

import { readStoredList, writeStoredList } from "./storedList";

/** One recorded workspace open. */
export interface RecentWorkspaceOpen {
  /** Workspace UUID. */
  id: string;
  /** ISO 8601 timestamp of the most recent open. */
  openedAt: string;
}

function isRecentOpen(entry: unknown): entry is RecentWorkspaceOpen {
  return (
    typeof entry === "object" &&
    entry !== null &&
    typeof (entry as { id?: unknown }).id === "string" &&
    typeof (entry as { openedAt?: unknown }).openedAt === "string"
  );
}

/** Recorded opens are capped at this many entries per user. */
export const RECENT_WORKSPACE_OPENS_CAP = 50;

/** Per-user localStorage key for recorded workspace opens. */
export function recentWorkspaceOpensKey(userId: string): string {
  return `g5:recentWorkspaces:opened:${userId}`;
}

/**
 * Pure list update: put `id` first with a fresh `openedAt`, dedupe by id, cap
 * at RECENT_WORKSPACE_OPENS_CAP. Does not touch storage.
 */
export function pushRecentWorkspaceOpen(
  opens: readonly RecentWorkspaceOpen[],
  id: string,
  openedAt: string,
): RecentWorkspaceOpen[] {
  return [{ id, openedAt }, ...opens.filter((entry) => entry.id !== id)].slice(
    0,
    RECENT_WORKSPACE_OPENS_CAP,
  );
}

/**
 * Read the most-recent-first open list for a user from localStorage.
 * Malformed or missing entries yield an empty list.
 */
export function readRecentWorkspaceOpens(
  userId: string,
): RecentWorkspaceOpen[] {
  return readStoredList(recentWorkspaceOpensKey(userId), isRecentOpen).slice(
    0,
    RECENT_WORKSPACE_OPENS_CAP,
  );
}

/**
 * Record a workspace open: prepend `workspaceId` (deduped, capped) and
 * persist. Returns the updated list. Storage failures are non-fatal — this is
 * fire-and-forget bookkeeping that must never break navigation.
 */
export function recordWorkspaceOpen(
  userId: string,
  workspaceId: string,
  now: () => string = () => new Date().toISOString(),
): RecentWorkspaceOpen[] {
  const next = pushRecentWorkspaceOpen(
    readRecentWorkspaceOpens(userId),
    workspaceId,
    now(),
  );
  writeStoredList(recentWorkspaceOpensKey(userId), next);
  return next;
}

/** Recorded open instants by workspace id. */
export function openedAtById(
  opens: readonly RecentWorkspaceOpen[],
): Map<string, string> {
  return new Map(opens.map((entry) => [entry.id, entry.openedAt]));
}

/**
 * The instant Home's "Recently opened" order and label use: the recorded
 * open in this browser, else the workspace's last update.
 */
export function workspaceOpenedAt(
  workspace: { id: string; lastUpdated: string },
  openedAt: ReadonlyMap<string, string>,
): { at: string; opened: boolean } {
  const recorded = openedAt.get(workspace.id);
  return recorded
    ? { at: recorded, opened: true }
    : { at: workspace.lastUpdated, opened: false };
}

/** Most recently opened (else updated) first. */
export function compareByRecentOpen(
  openedAt: ReadonlyMap<string, string>,
): (
  left: { id: string; lastUpdated: string },
  right: { id: string; lastUpdated: string },
) => number {
  const time = (workspace: { id: string; lastUpdated: string }) =>
    new Date(workspaceOpenedAt(workspace, openedAt).at).getTime() || 0;
  return (left, right) => time(right) - time(left);
}
