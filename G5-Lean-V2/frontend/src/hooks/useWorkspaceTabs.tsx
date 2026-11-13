// =============================================================================
// G5 Client — Workspace tabs provider
// =============================================================================
// Open workspaces as browser-style tabs (workbench g5.workbench.tabs). The
// route is the active tab: visiting a workspace route adds its tab, and the
// strip only navigates. The open-tab list is per-user localStorage
// (`lib/workspaceTabs.ts`); only the routed page is mounted. Showing a tab
// again returns to the URL it was last on, remembered in memory for this
// browser session only, so a reload still lands as a fresh open. The strip
// shows for a signed-in person on organization Home, workspace and
// fork-picker routes, every one of which names its
// organization: the strip shows that organization's open tabs, the other
// organizations' tabs staying in the list until their organization is shown
// again. A tab whose workspace is gone leaves the list: the deleting page
// closes it, the routed page drops a tab whose route no longer resolves, and
// the provider reconciles the other tabs with the organization's workspace
// list (g5.workbench.tabs.deleted). Without a provider the context is
// disabled, so pages render unchanged in isolation.
// =============================================================================

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";

import { useQuery } from "@tanstack/react-query";

import { useAuth } from "./useAuth";
import { workspacesQueryOptions } from "./useWorkspace";
import { classifyShellRoute, forkPickerPath } from "../lib/shellRoutes";
import {
  addWorkspaceTab,
  hasWorkspaceTab,
  moveWorkspaceTab,
  neighbourAfterClose,
  readWorkspaceTabs,
  removeWorkspaceTab,
  renameWorkspaceTab,
  sameWorkspaceTab,
  type WorkspaceTab,
  workspaceTabKey,
  workspaceTabPath,
  workspaceTabsKey,
  writeWorkspaceTabs,
} from "../lib/workspaceTabs";

/**
 * What the page remembers about a workspace tab for the browser session:
 * its open center tabs, the selected one and whether Chat was in front. The
 * page defines and reads the entries; the strip only keeps them.
 */
export interface WorkspaceTabSnapshot {
  /** The workspace the snapshot was taken in; a reused name does not inherit it. */
  workspaceId: string;
  tabs: readonly unknown[];
  activeTabId: string | null;
  chatCenter: boolean;
}

export type ActiveWorkspaceTab =
  | { kind: "home" }
  | {
      kind: "workspace";
      tab: WorkspaceTab;
      /**
       * The tab was shown earlier in this browser session: the page keeps
       * the surface it restores instead of landing afresh.
       */
      returned: boolean;
      /** The snapshot the page left for this tab, if any. */
      snapshot: WorkspaceTabSnapshot | null;
    }
  | { kind: "fork" }
  | { kind: "none" };

export interface WorkspaceTabsContextValue {
  /** Whether the strip renders: signed in, on a shell route. */
  enabled: boolean;
  /** The route organization's open workspace tabs in strip order. */
  tabs: WorkspaceTab[];
  /** The tab the current route selects. */
  active: ActiveWorkspaceTab;
  /** Organization the route is under: the tabs shown, Home and "+" are its. */
  orgSlug: string | null;
  /** Navigate to an open tab, at the URL it was last shown on this session. */
  selectTab: (tab: WorkspaceTab) => void;
  /** Keep the page's snapshot of a tab for the session; read back on a return. */
  rememberSnapshot: (tab: WorkspaceTab, snapshot: WorkspaceTabSnapshot) => void;
  /** Close a tab; when it is the active one, show its left neighbour, else Home. */
  closeTab: (tab: WorkspaceTab) => void;
  /**
   * The tab's workspace does not exist (g5.workbench.tabs.deleted): drop the
   * tab from the list without navigating. The routed page keeps showing
   * under a transient tab, so it can say the workspace was not found.
   */
  unlistTab: (tab: WorkspaceTab) => void;
  /**
   * A workspace was renamed: rewrite its tab in place, keeping its position
   * and session memory, and drop a duplicate under the other name
   * (g5.workbench.tabs). The page also moves a name tab onto its personal
   * workspace's kind-resolved route this way (g5.personal.routes).
   * Navigation is the caller's.
   */
  renameTab: (from: WorkspaceTab, to: WorkspaceTab) => void;
  /**
   * Move a tab to `index` among `tabs` (g5.workbench.tabs.reorder). The move
   * is applied to the latest list, so another browser tab's close is never
   * undone; navigation, selection and session memory are untouched.
   */
  moveTab: (tab: WorkspaceTab, index: number) => void;
  /** Navigate to the pinned Home tab. */
  openHome: () => void;
  /** Navigate to the route organization's fork picker ("+"). */
  openForkPicker: () => void;
}

const DISABLED: WorkspaceTabsContextValue = {
  enabled: false,
  tabs: [],
  active: { kind: "none" },
  orgSlug: null,
  selectTab: () => {},
  rememberSnapshot: () => {},
  closeTab: () => {},
  unlistTab: () => {},
  renameTab: () => {},
  moveTab: () => {},
  openHome: () => {},
  openForkPicker: () => {},
};

const WorkspaceTabsContext = createContext<WorkspaceTabsContextValue>(DISABLED);

/** A tab's session memory: where it was last shown and the page's snapshot. */
interface TabMemory {
  url: string;
  snapshot: WorkspaceTabSnapshot | null;
}

export function homePath(orgSlug: string): string {
  return `/${orgSlug}/home`;
}

export const WorkspaceTabsProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const { user } = useAuth();
  const userId = user?.id ?? "";
  const location = useLocation();
  const navigate = useNavigate();

  const route = useMemo(
    () => classifyShellRoute(location.pathname),
    [location.pathname],
  );
  const enabled = !!userId && route.kind !== "other";

  // The tab list belongs to the signed-in person; a sign-in/out swaps it.
  // `tabsRef` mirrors the state so a mutation reads the latest list and
  // persists exactly what it set, keeping the updater free of side effects.
  const [tabs, setTabs] = useState<WorkspaceTab[]>(() =>
    readWorkspaceTabs(userId),
  );
  const tabsRef = useRef(tabs);
  const replaceTabs = useCallback((next: WorkspaceTab[]) => {
    tabsRef.current = next;
    setTabs(next);
  }, []);
  useEffect(() => {
    replaceTabs(readWorkspaceTabs(userId));
  }, [replaceTabs, userId]);
  // Another browser tab of the same person changed the list.
  useEffect(() => {
    if (!userId || typeof window === "undefined") return;
    const key = workspaceTabsKey(userId);
    const onStorage = (event: StorageEvent) => {
      if (event.key === key) replaceTabs(readWorkspaceTabs(userId));
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [replaceTabs, userId]);

  const commit = useCallback(
    (update: (prev: WorkspaceTab[]) => WorkspaceTab[]) => {
      const next = update(tabsRef.current);
      replaceTabs(next);
      if (userId) writeWorkspaceTabs(userId, next);
    },
    [replaceTabs, userId],
  );

  // Arriving at a workspace route opens its tab (deep links, the switcher,
  // Home rows and forks all land here). Only an arrival does: when another
  // browser tab closes or evicts the routed workspace's tab, this one keeps
  // showing the page under a transient tab and does not reopen it, so a
  // close sticks in every browser tab (issue #461). Keyed by the workspace's
  // identity, not the route object, so navigating within the workspace is
  // not an arrival. Recorded only while the strip is shown so classic
  // layouts leave no trace.
  const routeOrgSlug = route.kind === "workspace" ? route.orgSlug : null;
  const routeWorkspaceName =
    route.kind === "workspace" ? route.workspaceName : null;
  const routePersonal =
    route.kind === "workspace" ? (route.personal ?? null) : null;
  const routeTab = useMemo<WorkspaceTab | null>(
    () =>
      routeOrgSlug !== null && routeWorkspaceName !== null
        ? {
            orgSlug: routeOrgSlug,
            name: routeWorkspaceName,
            ...(routePersonal ? { personal: routePersonal } : {}),
          }
        : null,
    [routeOrgSlug, routeWorkspaceName, routePersonal],
  );
  // A page can redirect a route before its arrival is recorded: with the
  // workspace already cached, the page's effect runs first (a child's
  // effects precede its parent's), moves the tab away from the routed name
  // through `renameTab` and navigates on, and only then does this effect
  // run for the name it left. `renameTab` notes the name it moved from,
  // and that one pending arrival is skipped, so the redirected route never
  // opens a tab of its own beside the one the page moved to.
  const redirectedArrivalRef = useRef<string | null>(null);
  // The routed tab as of the latest render, for `renameTab` to tell a
  // redirect of the routed name from a rename of another open tab.
  const routeTabRef = useRef(routeTab);
  routeTabRef.current = routeTab;
  useEffect(() => {
    // The note is for the next arrival only, whatever route that is.
    const redirected = redirectedArrivalRef.current;
    redirectedArrivalRef.current = null;
    if (!enabled || !routeTab) return;
    if (redirected === workspaceTabKey(routeTab)) return;
    if (hasWorkspaceTab(tabsRef.current, routeTab)) return;
    commit((prev) => addWorkspaceTab(prev, routeTab));
  }, [commit, enabled, routeTab]);

  // Every shell route names its organization; the strip is scoped to it.
  const orgSlug = route.kind === "other" ? null : route.orgSlug;
  // The routed workspace whose tab was closed elsewhere shows as a
  // transient tab at the end, never written to the list.
  const orgTabs = useMemo(() => {
    const listed = tabs.filter((tab) => tab.orgSlug === orgSlug);
    return enabled && routeTab ? addWorkspaceTab(listed, routeTab) : listed;
  }, [enabled, orgSlug, routeTab, tabs]);

  // Where each open tab was last shown — its full URL — and the snapshot
  // the page left for it, keyed by the tab's identity (`workspaceTabKey`,
  // so `/acme/base` and `/acme/Base` share one memory). Memory for this browser
  // session only — a reload starts empty, so the page lands afresh — and
  // dropped when the tab closes. The routed workspace's URL is recorded on
  // every route change, so a doc, `?view=` or `?session=` the page writes
  // is kept.
  const [memory, setMemory] = useState<ReadonlyMap<string, TabMemory>>(
    () => new Map(),
  );
  const routeTabKey = routeTab ? workspaceTabKey(routeTab) : null;
  // `visit` says whether this visit is a return: a navigation the strip
  // itself made (`selectTab`, `closeTab`, marked in the history entry's
  // state) to a tab it has memory for. A link, Home row or address-bar
  // deep link to the same workspace is a fresh open, so what it names is
  // shown. Derived during render when the routed tab changes — before the
  // effect below records this visit's URL — so the page sees it in the
  // same commit as the route, from its first effects on. A visit is keyed
  // by the name as routed (`workspaceTabPath`), so a link to the open
  // workspace under another spelling of its name is a fresh open of the
  // same tab, while navigation within the workspace is not a new visit.
  const returnNavigation =
    (location.state as { workspaceTabReturn?: boolean } | null)
      ?.workspaceTabReturn === true;
  const routeVisitKey = routeTab ? workspaceTabPath(routeTab) : null;
  const [visit, setVisit] = useState<{ key: string | null; returned: boolean }>(
    { key: null, returned: false },
  );
  if (visit.key !== routeVisitKey) {
    setVisit({
      key: routeVisitKey,
      returned:
        routeTabKey !== null && returnNavigation && memory.has(routeTabKey),
    });
  }
  const returned = visit.key === routeVisitKey && visit.returned;
  const snapshot = routeTabKey
    ? (memory.get(routeTabKey)?.snapshot ?? null)
    : null;
  useEffect(() => {
    if (!routeTabKey) return;
    const url = `${location.pathname}${location.search}${location.hash}`;
    setMemory((prev) => {
      const current = prev.get(routeTabKey);
      if (current?.url === url) return prev;
      return new Map(prev).set(routeTabKey, {
        url,
        snapshot: current?.snapshot ?? null,
      });
    });
  }, [location.hash, location.pathname, location.search, routeTabKey]);
  // A tab that leaves the list — closed here or in another browser tab,
  // or evicted by the cap — keeps no memory. The routed workspace's URL is
  // kept while it is shown (an arrival renders before its tab is listed);
  // a transient tab takes no snapshot, and leaving it drops the URL.
  useEffect(() => {
    setMemory((prev) => {
      let next: Map<string, TabMemory> | null = null;
      for (const key of prev.keys()) {
        if (key === routeTabKey) continue;
        if (tabs.some((tab) => workspaceTabKey(tab) === key)) continue;
        next ??= new Map(prev);
        next.delete(key);
      }
      return next ?? prev;
    });
  }, [routeTabKey, tabs]);
  const rememberSnapshot = useCallback(
    (tab: WorkspaceTab, next: WorkspaceTabSnapshot) => {
      // A page leaving a tab that was just closed still flushes its write;
      // a closed tab keeps no memory.
      if (!hasWorkspaceTab(tabsRef.current, tab)) return;
      const key = workspaceTabKey(tab);
      setMemory((prev) =>
        new Map(prev).set(key, {
          url: prev.get(key)?.url ?? workspaceTabPath(tab),
          snapshot: next,
        }),
      );
    },
    [],
  );
  // The memory belongs to the signed-in person, like the tab list: a
  // sign-out or a switch of person empties it. The id arriving after the
  // first route is not a switch.
  const lastUrlsUserRef = useRef("");
  useEffect(() => {
    if (lastUrlsUserRef.current && lastUrlsUserRef.current !== userId) {
      setMemory(new Map());
      setVisit({ key: null, returned: false });
    }
    lastUrlsUserRef.current = userId;
  }, [userId]);
  // Navigate to a tab: at its remembered URL, marked as a return, when the
  // strip has memory for it; at its root otherwise.
  const navigateToTab = useCallback(
    (tab: WorkspaceTab) => {
      const path = workspaceTabPath(tab);
      const remembered = memory.get(workspaceTabKey(tab));
      if (remembered) {
        navigate(remembered.url, { state: { workspaceTabReturn: true } });
      } else {
        navigate(path);
      }
    },
    [memory, navigate],
  );

  const active = useMemo<ActiveWorkspaceTab>(() => {
    if (route.kind === "home") return { kind: "home" };
    if (route.kind === "fork") return { kind: "fork" };
    if (routeTab) {
      return { kind: "workspace", tab: routeTab, returned, snapshot };
    }
    return { kind: "none" };
  }, [returned, route.kind, routeTab, snapshot]);

  const openHome = useCallback(() => {
    if (orgSlug) navigate(homePath(orgSlug));
  }, [navigate, orgSlug]);

  const selectTab = useCallback(
    (tab: WorkspaceTab) => {
      if (routeTab && sameWorkspaceTab(routeTab, tab)) return;
      navigateToTab(tab);
    },
    [navigateToTab, routeTab],
  );

  const closeTab = useCallback(
    (tab: WorkspaceTab) => {
      const isActive = !!routeTab && sameWorkspaceTab(routeTab, tab);
      // The neighbour is in the strip, which shows the active tab — listed
      // or transient — in the route organization.
      const neighbour = isActive ? neighbourAfterClose(orgTabs, tab) : null;
      commit((prev) => removeWorkspaceTab(prev, tab));
      if (!isActive) return;
      if (neighbour) navigateToTab(neighbour);
      else navigate(homePath(tab.orgSlug));
    },
    [commit, navigate, navigateToTab, orgTabs, routeTab],
  );

  const renameTab = useCallback(
    (from: WorkspaceTab, to: WorkspaceTab) => {
      if (
        from.orgSlug === to.orgSlug &&
        from.name === to.name &&
        from.personal === to.personal
      ) {
        return;
      }
      // The routed name's arrival may still be pending (the page redirected
      // from a cached first render): whether or not `from` is listed, the
      // name is now `to`, so that arrival is skipped above rather than
      // reopening the name it left. Only the routed tab's rename is such a
      // redirect; a rename reconciled for another open tab leaves a later
      // direct arrival at its old name — a new workspace under it — alone.
      const routed = routeTabRef.current;
      if (routed && sameWorkspaceTab(from, routed)) {
        redirectedArrivalRef.current = workspaceTabKey(from);
      }
      commit((prev) => renameWorkspaceTab(prev, from, to));
      // The memory follows the tab: re-key it by the folded identity and
      // rewrite the workspace segment of the remembered URL. A case-only
      // rename keeps the key. Memory already under the new name wins; it was
      // shown as that workspace most recently. `from` is a name tab — a
      // personal workspace is never renamed — so the remembered path's
      // second segment is its name; a move onto the personal route
      // (g5.personal.routes) hangs the rest of the path under that route.
      const fromKey = workspaceTabKey(from);
      const toKey = workspaceTabKey(to);
      setMemory((prev) => {
        const moved = prev.get(fromKey);
        if (!moved) return prev;
        const next = new Map(prev);
        next.delete(fromKey);
        if (!next.has(toKey)) {
          const tail = moved.url.search(/[?#]/);
          const path = tail === -1 ? moved.url : moved.url.slice(0, tail);
          const rest = tail === -1 ? "" : moved.url.slice(tail);
          const segments = path.split("/");
          let url = moved.url;
          if (segments.length > 2) {
            if (to.personal) {
              url = [workspaceTabPath(to), ...segments.slice(3)].join("/");
            } else {
              segments[2] = encodeURIComponent(to.name);
              url = segments.join("/");
            }
            url += rest;
          }
          next.set(toKey, { url, snapshot: moved.snapshot });
        }
        return next;
      });
    },
    [commit],
  );

  const unlistTab = useCallback(
    (tab: WorkspaceTab) => {
      if (!hasWorkspaceTab(tabsRef.current, tab)) return;
      commit((prev) => removeWorkspaceTab(prev, tab));
    },
    [commit],
  );

  // A workspace deleted elsewhere — in another browser, on another device
  // or by another person — sends no event; the shell learns of it from the
  // server (g5.workbench.tabs.deleted). The routed page answers for its own
  // workspace. For the other tabs the answer is the organization's workspace
  // list, the query Home and the strip's glyphs read: a workspace this
  // session saw listed that a later list lacks is gone from the person's
  // view — deleted, renamed or no longer visible. A rename is told apart by
  // the id, still listed under another name, and the tab follows it
  // (g5.workbench.tabs.rename); otherwise the tab leaves the list. Only a
  // workspace once seen counts: the list never shows some workspaces a tab
  // can open (such as personal workspaces omitted by a discovery surface), and a list fetched before a fork or creation lacks the
  // workspace just opened, so absence alone says nothing. Personal tabs
  // resolve by kind, not by name, and are never owner-deleted.
  const orgId =
    user?.orgs.find((membership) => membership.orgSlug === orgSlug)?.orgId ??
    "";
  const { data: listedWorkspaces } = useQuery({
    ...workspacesQueryOptions(orgId),
    enabled: enabled && !!orgId,
  });
  const seenWorkspaceIdsRef = useRef(new Map<string, string>());
  useEffect(() => {
    if (!listedWorkspaces || !orgSlug) return;
    const byName = new Map(
      listedWorkspaces.map((workspace) => [
        workspace.name.toLowerCase(),
        workspace,
      ]),
    );
    const byId = new Map(
      listedWorkspaces.map((workspace) => [workspace.id, workspace]),
    );
    const seen = seenWorkspaceIdsRef.current;
    for (const tab of [...tabsRef.current]) {
      if (tab.orgSlug !== orgSlug || tab.personal) continue;
      const key = workspaceTabKey(tab);
      const listed = byName.get(tab.name.toLowerCase());
      if (listed) {
        seen.set(key, listed.id);
        continue;
      }
      const seenId = seen.get(key);
      if (!seenId || (routeTab && sameWorkspaceTab(tab, routeTab))) continue;
      seen.delete(key);
      const renamed = byId.get(seenId);
      if (renamed) {
        const to = { orgSlug, name: renamed.name };
        seen.set(workspaceTabKey(to), renamed.id);
        renameTab(tab, to);
      } else {
        unlistTab(tab);
      }
    }
  }, [listedWorkspaces, orgSlug, renameTab, routeTab, unlistTab]);

  const moveTab = useCallback(
    (tab: WorkspaceTab, index: number) => {
      // The strip may show a transient tab, which is not in the list: the
      // move is anchored on the tabs shown to the moved tab's right, and the
      // first of them still listed takes the place.
      const shown = orgTabs.filter((entry) => !sameWorkspaceTab(entry, tab));
      const followers = shown.slice(Math.max(0, index));
      // Another browser tab may have written the list after the last storage
      // event reached this one: the move applies to what is stored now, so a
      // tab closed there is not written back. A tab no longer stored moves
      // nothing.
      const latest = userId ? readWorkspaceTabs(userId) : tabsRef.current;
      if (!hasWorkspaceTab(latest, tab)) return;
      commit(() => moveWorkspaceTab(latest, tab, followers));
    },
    [commit, orgTabs, userId],
  );

  const openForkPicker = useCallback(() => {
    if (orgSlug && route.kind !== "fork") navigate(forkPickerPath(orgSlug));
  }, [navigate, orgSlug, route.kind]);

  const value = useMemo<WorkspaceTabsContextValue>(
    () => ({
      enabled,
      tabs: orgTabs,
      active,
      orgSlug,
      selectTab,
      rememberSnapshot,
      closeTab,
      unlistTab,
      renameTab,
      moveTab,
      openHome,
      openForkPicker,
    }),
    [
      active,
      closeTab,
      enabled,
      moveTab,
      openForkPicker,
      openHome,
      orgSlug,
      orgTabs,
      rememberSnapshot,
      renameTab,
      selectTab,
      unlistTab,
    ],
  );

  return (
    <WorkspaceTabsContext.Provider value={value}>
      {children}
    </WorkspaceTabsContext.Provider>
  );
};

/** The workspace tab strip state; disabled outside a provider. */
export function useWorkspaceTabs(): WorkspaceTabsContextValue {
  return useContext(WorkspaceTabsContext);
}
