// =============================================================================
// G5 Client — Open workspace tabs
// =============================================================================
// Per-browser, per-user localStorage list of the workspaces a person keeps
// open as tabs in the tab strip (workbench g5.workbench.tabs). Entries carry
// the route identity only — organization slug and workspace name — so nothing
// in storage can go stale: the strip shows the name and the route resolves the
// rest. Follows the recent-opens idiom (`recentWorkspaces.ts`) through the
// shared tolerant list store (`storedList.ts`).
// =============================================================================

import { readStoredList, writeStoredList } from "./storedList";
import {
  personalWorkspaceMePath,
  personalWorkspaceUserPath,
  workspacePagePath,
} from "./workspacePagePath";

/** One open workspace tab: the route identity of a workspace. */
export interface WorkspaceTab {
  /** Organization route slug. */
  orgSlug: string;
  /**
   * Workspace name (the route segment, decoded); `me` on the caller's own
   * personal workspace route, `users/<handle>` on a member's.
   */
  name: string;
  /**
   * A personal workspace route (g5.personal.routes): it resolves by kind,
   * so the tab keeps that route rather than the name route.
   */
  personal?: "me" | "user";
}

const PERSONAL_NAME_PREFIX = "users/";

/** Open tabs are capped at this many entries per user. */
export const WORKSPACE_TABS_CAP = 30;

/** Per-user localStorage key for the open tab list. */
export function workspaceTabsKey(userId: string): string {
  return `g5:workspaceTabs:${userId}`;
}

/** Route path a tab navigates to, with the name as it was routed. */
export function workspaceTabPath(tab: WorkspaceTab): string {
  if (tab.personal === "me") return personalWorkspaceMePath(tab.orgSlug);
  if (tab.personal === "user" && tab.name.startsWith(PERSONAL_NAME_PREFIX)) {
    return personalWorkspaceUserPath(
      tab.orgSlug,
      tab.name.slice(PERSONAL_NAME_PREFIX.length),
    );
  }
  return workspacePagePath(tab.orgSlug, tab.name);
}

/** What the strip shows for a tab until its workspace is listed. */
export function workspaceTabLabel(tab: WorkspaceTab): string {
  return tab.personal === "me" ? "Personal workspace" : tab.name;
}

/**
 * A tab's stable identity: its organization and workspace name as the route
 * resolves them. The server matches workspace names case-insensitively
 * (`/acme/base` and `/acme/Base` open the one Base workspace), so the name
 * is folded the same way: one workspace is one tab however it was reached.
 * A personal route resolves by kind, so it is a different tab from an
 * ordinary workspace whose name looks personal.
 */
export function workspaceTabKey(tab: WorkspaceTab): string {
  const route = tab.personal ? `${tab.personal}:` : "";
  return `${tab.orgSlug}/${route}${tab.name.toLowerCase()}`;
}

export function sameWorkspaceTab(a: WorkspaceTab, b: WorkspaceTab): boolean {
  return workspaceTabKey(a) === workspaceTabKey(b);
}

export function hasWorkspaceTab(
  tabs: readonly WorkspaceTab[],
  tab: WorkspaceTab,
): boolean {
  return tabs.some((entry) => sameWorkspaceTab(entry, tab));
}

/**
 * Pure list update: append `tab` at the end unless it is already open; cap at
 * WORKSPACE_TABS_CAP by dropping the leftmost tabs (the oldest, unless the
 * person reordered them). Order is the strip order, so an existing tab keeps
 * its place.
 */
export function addWorkspaceTab(
  tabs: readonly WorkspaceTab[],
  tab: WorkspaceTab,
): WorkspaceTab[] {
  if (hasWorkspaceTab(tabs, tab)) return [...tabs];
  return [...tabs, tab].slice(-WORKSPACE_TABS_CAP);
}

/**
 * Pure list update for a rename (g5.workbench.tabs): the tab for `from` is
 * rewritten in place as `to`, keeping its position; a case-only rename
 * rewrites the label of the same tab. When both names are open, the earlier
 * position is kept and the other entry dropped. A list without `from` is
 * returned unchanged. The same move takes a name tab onto its workspace's
 * kind-resolved personal route (g5.personal.routes): `to` keeps its route
 * kind, so the personal workspace stays one tab.
 */
export function renameWorkspaceTab(
  tabs: readonly WorkspaceTab[],
  from: WorkspaceTab,
  to: WorkspaceTab,
): WorkspaceTab[] {
  const fromIndex = tabs.findIndex((entry) => sameWorkspaceTab(entry, from));
  if (fromIndex === -1) return [...tabs];
  // A case-only rename is the same tab: rewrite its name in place.
  const toIndex = tabs.findIndex((entry) => sameWorkspaceTab(entry, to));
  const keep = toIndex === -1 ? fromIndex : Math.min(fromIndex, toIndex);
  const renamed: WorkspaceTab = {
    orgSlug: to.orgSlug,
    name: to.name,
    ...(to.personal ? { personal: to.personal } : {}),
  };
  return tabs.flatMap((entry, index) => {
    if (index === keep) return [renamed];
    if (index === fromIndex || index === toIndex) return [];
    return [entry];
  });
}

/**
 * Pure list update for a reorder (g5.workbench.tabs.reorder): move the open
 * `tab` in front of the first of `followers` — the tabs the strip shows to
 * its right after the drop, nearest first — that is still open in its
 * organization, else behind that organization's last open tab. The stored
 * entry moves, so its route kind is kept, and other organizations' tabs keep
 * their places. A `tab` no longer open (closed or renamed elsewhere) is not
 * written back: the list is returned unchanged.
 */
export function moveWorkspaceTab(
  tabs: readonly WorkspaceTab[],
  tab: WorkspaceTab,
  followers: readonly WorkspaceTab[],
): WorkspaceTab[] {
  const from = tabs.findIndex((entry) => sameWorkspaceTab(entry, tab));
  if (from === -1) return [...tabs];
  const moved = tabs[from];
  const rest = tabs.filter((_, index) => index !== from);
  const anchor = followers
    .filter(
      (follower) =>
        follower.orgSlug === moved.orgSlug &&
        !sameWorkspaceTab(follower, moved),
    )
    .map((follower) =>
      rest.findIndex((entry) => sameWorkspaceTab(entry, follower)),
    )
    .find((index) => index !== -1);
  let to = anchor;
  if (to === undefined) {
    const last = rest.findLastIndex((entry) => entry.orgSlug === moved.orgSlug);
    if (last === -1) return [...tabs];
    to = last + 1;
  }
  return [...rest.slice(0, to), moved, ...rest.slice(to)];
}

/** Pure list update: drop `tab` if present. */
export function removeWorkspaceTab(
  tabs: readonly WorkspaceTab[],
  tab: WorkspaceTab,
): WorkspaceTab[] {
  return tabs.filter((entry) => !sameWorkspaceTab(entry, tab));
}

/**
 * The tab to show after closing `closed` while it is active: the tab on its
 * left, else none — the caller goes Home, which is pinned to the left of
 * every workspace tab (mock story B1).
 */
export function neighbourAfterClose(
  tabs: readonly WorkspaceTab[],
  closed: WorkspaceTab,
): WorkspaceTab | null {
  const index = tabs.findIndex((entry) => sameWorkspaceTab(entry, closed));
  return index > 0 ? tabs[index - 1] : null;
}

function isWorkspaceTab(value: unknown): value is WorkspaceTab {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { orgSlug?: unknown }).orgSlug === "string" &&
    (value as { orgSlug: string }).orgSlug.length > 0 &&
    typeof (value as { name?: unknown }).name === "string" &&
    (value as { name: string }).name.length > 0 &&
    [undefined, "me", "user"].includes(
      (value as { personal?: unknown }).personal as string | undefined,
    )
  );
}

/**
 * Read a user's open tabs from localStorage. Malformed or missing entries
 * yield an empty list; duplicates collapse to their first occurrence.
 */
export function readWorkspaceTabs(userId: string): WorkspaceTab[] {
  return readStoredList(workspaceTabsKey(userId), isWorkspaceTab)
    .reduce<WorkspaceTab[]>((tabs, tab) => addWorkspaceTab(tabs, tab), [])
    .slice(0, WORKSPACE_TABS_CAP);
}

/** Persist a user's open tabs; storage failures are non-fatal. */
export function writeWorkspaceTabs(
  userId: string,
  tabs: readonly WorkspaceTab[],
): void {
  writeStoredList(workspaceTabsKey(userId), tabs);
}
