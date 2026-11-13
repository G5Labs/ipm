// =============================================================================
// G5 Client — recorded workspace opens, live
// =============================================================================
// The per-browser open record (`lib/recentWorkspaces.ts`) as React state:
// re-read when another tab records an open (the `storage` event; this tab's
// own opens happen on the workspace page, after Home unmounts), so Home's
// "Recently opened" order never sits on a stale snapshot.
// =============================================================================

import { useMemo, useSyncExternalStore } from "react";

import {
  readRecentWorkspaceOpens,
  recentWorkspaceOpensKey,
  type RecentWorkspaceOpen,
} from "../lib/recentWorkspaces";

function subscribe(onChange: () => void): () => void {
  window.addEventListener("storage", onChange);
  return () => window.removeEventListener("storage", onChange);
}

export function useRecentWorkspaceOpens(userId: string): RecentWorkspaceOpen[] {
  const key = recentWorkspaceOpensKey(userId);
  // The raw string is the change signal: an unrelated key leaves it equal.
  const raw = useSyncExternalStore(
    subscribe,
    () => {
      try {
        return localStorage.getItem(key);
      } catch {
        return null;
      }
    },
    () => null,
  );
  return useMemo(() => {
    void raw;
    return userId ? readRecentWorkspaceOpens(userId) : [];
  }, [userId, raw]);
}
