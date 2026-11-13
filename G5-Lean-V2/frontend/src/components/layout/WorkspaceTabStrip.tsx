// =============================================================================
// G5 Client — Workspace tab strip
// =============================================================================
// Browser-style strip above every shell page under the alignment layout
// (workbench g5.workbench.tabs; mock stories B1–B4): a pinned Home tab, one
// closable tab per workspace open in the route's organization, a transient
// "New tab" while the fork picker is routed, "+" which opens the picker, and at the right end the
// page's global controls (`trailing`; g5.workbench.shell-chrome). Presentation only — the
// state and navigation live in `useWorkspaceTabs`. Each workspace tab is a
// `role=tab` button beside a sibling close button (a button may not contain
// another control); Left/Right arrows move focus along the tablist. Home stays
// outside the sideways scroller, so overflowing tabs never hide it. Workspace
// tabs reorder by dragging or Ctrl/Cmd+Shift+Left/Right
// (g5.workbench.tabs.reorder); Home, the New tab and "+" are outside that
// range. A drag shifts the other tabs in place rather than moving them in the
// DOM, so the pressed tab keeps its pointer capture, and the move is committed
// on release. A tab's
// glyph is its workspace's phase icon (g5.workbench.phases), read from the
// organization's workspace list — the one Home already loads — so a phase
// move shows on the strip once that list refreshes. The list is matched by
// name as the server resolves a route, case-insensitively, so a tab reached
// at `/:org/base` finds the listed `Base` and shows that listed name. A
// personal workspace tab is not in that list; in the glyph's place it shows
// its owner's round avatar (the caller's own from `useAuth`, another member's
// resolved through the organization roster), reusing `IdentityAvatar` so the
// same person keeps the same picture or initial-and-tint fallback as on Home.
// =============================================================================

import { useQuery } from "@tanstack/react-query";
import { House, Plus, X } from "lucide-react";
import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { useAuth } from "../../hooks/useAuth";
import { useOrgMembers } from "../../hooks/useOrg";
import {
  personalWorkspaceQueryOptions,
  workspacesQueryOptions,
} from "../../hooks/useWorkspace";
import { useWorkspaceTabs } from "../../hooks/useWorkspaceTabs";
import type { WorkspacePhase } from "../../lib/types";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import {
  sameWorkspaceTab,
  workspaceTabKey,
  workspaceTabLabel,
  workspaceTabPath,
  type WorkspaceTab,
} from "../../lib/workspaceTabs";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import { classNames } from "../ui/UI";
import { PHASE_ICONS } from "../workbench/phaseIcons";

/** Avatar size on a personal tab: the phase icon's 13 px box plus one. */
const TAB_AVATAR_SIZE = 14;

/** The `users/<handle>` name prefix a member's personal route carries. */
const PERSONAL_USER_PREFIX = "users/";

const TAB_GROUP =
  "relative inline-flex h-8 max-w-[220px] min-w-0 shrink-0 items-center rounded-t-lg border border-b-0 transition-colors duration-150";
const TAB_GROUP_IDLE =
  "border-transparent text-[var(--g5-text2)] hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text1)]";
const TAB_GROUP_ACTIVE =
  "z-[1] border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[var(--g5-text0)] after:absolute after:inset-x-0 after:-bottom-px after:h-px after:bg-[var(--g5-bg0)]";
const FOCUS_RING =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";
const TAB_BUTTON =
  "inline-flex h-full min-w-0 items-center gap-2 rounded-t-lg px-3 text-xs cursor-pointer " +
  FOCUS_RING;
const TAB_GROUP_DRAGGED =
  "z-[2] cursor-grabbing shadow-[0_2px_8px_rgba(0,0,0,0.25)] [&_*]:cursor-grabbing";

/** A press moves this far sideways before it is a drag; less is a click. */
export const TAB_DRAG_THRESHOLD = 5;
/** Within this far of the scroller's edge a drag scrolls the strip. */
const TAB_DRAG_EDGE = 40;
/** The fastest auto-scroll, in pixels per frame, at or past the edge. */
const TAB_DRAG_SCROLL_STEP = 16;

export interface WorkspaceTabStripProps {
  /** Global controls at the strip's right end (g5.workbench.shell-chrome). */
  trailing?: React.ReactNode;
}

/** What the strip shows for a listed workspace: its name as listed, its phase. */
interface ListedWorkspace {
  name: string;
  phase: WorkspacePhase;
}

/**
 * The route organization's listed workspaces by lower-cased name — the
 * server resolves a route's workspace name case-insensitively, so the strip
 * matches the same way. The list is the organization's workspace query
 * (shared with Home); a slug outside the caller's memberships, a list not
 * yet loaded or a name the list lacks (closed elsewhere, deleted) yields
 * nothing. An older server omits `phase`; the DTO default is `be`, as the
 * chevrons assume.
 */
function useListedWorkspacesByName(
  orgSlug: string | null,
): ReadonlyMap<string, ListedWorkspace> {
  const { user } = useAuth();
  const orgId =
    user?.orgs.find((candidate) => candidate.orgSlug === orgSlug)?.orgId ?? "";
  const { data } = useQuery(workspacesQueryOptions(orgId));
  return useMemo(
    () =>
      new Map(
        (data ?? []).map((workspace) => [
          workspace.name.toLowerCase(),
          { name: workspace.name, phase: workspace.phase ?? "be" },
        ]),
      ),
    [data],
  );
}

/**
 * A tab's glyph: the phase icon in its hue, or an empty slot until known.
 * The tab's accessible name stays the workspace name; the phase is the
 * glyph's tooltip.
 */
const WorkspaceTabIcon: React.FC<{ phase: WorkspacePhase | undefined }> = ({
  phase,
}) => {
  if (!phase) {
    return <span aria-hidden="true" className="h-[13px] w-[13px] shrink-0" />;
  }
  const Icon = PHASE_ICONS[phase];
  const info = WORKBENCH_PHASES[phase];
  return (
    <span
      data-testid="workspace-tab-phase"
      data-phase={phase}
      title={info.name}
      className="inline-flex shrink-0"
      style={{ color: `var(${info.token})` }}
    >
      <Icon size={13} />
    </span>
  );
};

/**
 * The caller's own personal tab (`tab.personal === "me"`) shows the signed-in
 * person's avatar — the OAuth picture, else `IdentityAvatar`'s deterministic
 * initial-and-tint fallback from their email — read from `useAuth`, which the
 * strip already has. The tab's accessible name stays its label; the avatar is
 * decorative with the display name as its tooltip.
 */
const OwnTabAvatar: React.FC = () => {
  const { user } = useAuth();
  if (!user) {
    return (
      <span
        aria-hidden="true"
        className="shrink-0"
        style={{ width: TAB_AVATAR_SIZE, height: TAB_AVATAR_SIZE }}
      />
    );
  }
  return (
    <IdentityAvatar
      identity={user.email}
      avatarUrl={user.avatarUrl}
      title={user.displayName}
      size={TAB_AVATAR_SIZE}
    />
  );
};

/**
 * A member's personal tab (`/:org/users/:handle`) shows that member's avatar.
 * The handle resolves to the member's user id through the personal workspace
 * endpoint — the same query the page itself resolves by, so it shares the
 * cache — and the organization roster carries the avatar and display name, as
 * on Home. Until it resolves (or where it cannot, for a caller who is not the
 * owner or an administrator) the fallback is keyed on the handle, so the slot
 * is never empty and the colour is stable.
 */
const MemberTabAvatar: React.FC<{ orgSlug: string; handle: string }> = ({
  orgSlug,
  handle,
}) => {
  const { user } = useAuth();
  const orgId =
    user?.orgs.find((candidate) => candidate.orgSlug === orgSlug)?.orgId ?? "";
  const { data: personal } = useQuery(
    personalWorkspaceQueryOptions(orgId, handle),
  );
  const membersState = useOrgMembers(orgId);
  const members = membersState.status === "ok" ? membersState.data : undefined;
  const member = personal
    ? members?.find((candidate) => candidate.userId === personal.userId)
    : undefined;
  return (
    <IdentityAvatar
      identity={member?.email ?? handle}
      avatarUrl={member?.avatarUrl}
      title={member?.displayName ?? handle}
      size={TAB_AVATAR_SIZE}
    />
  );
};

/** The glyph a tab shows: a personal tab's owner avatar, else its phase icon. */
const WorkspaceTabGlyph: React.FC<{
  tab: WorkspaceTab;
  phase: WorkspacePhase | undefined;
}> = ({ tab, phase }) => {
  if (tab.personal === "me") return <OwnTabAvatar />;
  if (tab.personal === "user") {
    const handle = tab.name.startsWith(PERSONAL_USER_PREFIX)
      ? tab.name.slice(PERSONAL_USER_PREFIX.length)
      : tab.name;
    return <MemberTabAvatar orgSlug={tab.orgSlug} handle={handle} />;
  }
  return <WorkspaceTabIcon phase={phase} />;
};

/** Where the workspace tabs sat when a drag began, in scroller content pixels. */
interface TabLayout {
  lefts: number[];
  widths: number[];
  shift: number;
}

/** The drag the strip renders: who is dragged, from and to where. */
interface TabDrag {
  key: string;
  from: number;
  index: number;
  /** The dragged tab's offset from where it sat. */
  offset: number;
  /** How far a passed tab moves aside: the dragged tab's width and the gap. */
  shift: number;
  /** The tab keys the drag measured. */
  keys: string;
}

/** A press on a workspace tab, from pointer-down to release. */
interface TabPress {
  pointerId: number;
  key: string;
  from: number;
  /** Where the dragged tab would land now. */
  index: number;
  startX: number;
  clientX: number;
  /** The tab keys when the press began; any change cancels the drag. */
  keys: string;
  dragging: boolean;
  /** Escape or a list change ended the drag; the release only swallows its click. */
  cancelled: boolean;
  /** Where in the tab the pointer took hold, in pixels from its left edge. */
  grab: number;
  layout: TabLayout | null;
}

/** The tabs' identities in strip order, as one comparable value. */
function tabKeys(tabs: readonly WorkspaceTab[]): string {
  return tabs.map(workspaceTabKey).join("\n");
}

/**
 * Pointer and keyboard reordering of the workspace tabs. A drag measures the
 * tabs once, then keeps the dragged tab under the pointer, within the tabs'
 * span, and shifts the tabs it passes aside. Near either edge of the
 * scroller the strip scrolls, so overflowing tabs are reachable.
 */
function useTabReorder(
  tabs: readonly WorkspaceTab[],
  moveTab: (tab: WorkspaceTab, index: number) => void,
) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const pressRef = useRef<TabPress | null>(null);
  const suppressClickRef = useRef(false);
  const frameRef = useRef<number | null>(null);
  const [measuredDrag, setDrag] = useState<TabDrag | null>(null);
  const keys = tabKeys(tabs);
  // The list changed under the drag (another browser tab closed or opened a
  // tab): the measured places no longer hold, so the drag is over and its
  // release moves nothing.
  const drag = measuredDrag?.keys === keys ? measuredDrag : null;
  const tabsRef = useRef(tabs);
  useLayoutEffect(() => {
    tabsRef.current = tabs;
  }, [tabs]);

  const stopScrolling = useCallback(() => {
    if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    frameRef.current = null;
  }, []);

  /** Place the dragged tab under the pointer and pick its landing slot. */
  const track = useCallback(() => {
    const press = pressRef.current;
    const scroller = scrollerRef.current;
    if (!press?.layout || press.cancelled || !scroller) return;
    if (press.keys !== tabKeys(tabsRef.current)) {
      press.cancelled = true;
      setDrag(null);
      return;
    }
    const { lefts, widths } = press.layout;
    const box = scroller.getBoundingClientRect();
    const width = widths[press.from];
    const firstLeft = lefts[0];
    const lastRight = lefts[lefts.length - 1] + widths[widths.length - 1];
    const left = Math.min(
      Math.max(
        press.clientX - box.left + scroller.scrollLeft - press.grab,
        firstLeft,
      ),
      lastRight - width,
    );
    // The dragged tab passes a neighbour once its leading edge crosses the
    // neighbour's midpoint, so it reaches either end even though it never
    // leaves the tabs' span.
    let index = 0;
    lefts.forEach((other, i) => {
      const middle = other + widths[i] / 2;
      if (
        i < press.from
          ? left >= middle
          : i > press.from && left + width > middle
      ) {
        index += 1;
      }
    });
    press.index = index;
    setDrag({
      key: press.key,
      from: press.from,
      index,
      offset: left - lefts[press.from],
      shift: press.layout.shift,
      keys: press.keys,
    });
  }, []);

  // Near an edge the strip scrolls a little every frame, faster the closer
  // the pointer is, until the drag ends, the pointer moves back in or the
  // strip can scroll no further.
  const scrollNearEdge = useCallback(() => {
    if (frameRef.current !== null) return;
    const tick = () => {
      frameRef.current = null;
      const press = pressRef.current;
      const scroller = scrollerRef.current;
      if (!press?.dragging || press.cancelled || !scroller) return;
      const box = scroller.getBoundingClientRect();
      const intoLeft = box.left + TAB_DRAG_EDGE - press.clientX;
      const intoRight = press.clientX - (box.right - TAB_DRAG_EDGE);
      const depth = Math.max(intoLeft, intoRight);
      if (depth <= 0) return;
      const step = Math.ceil(
        TAB_DRAG_SCROLL_STEP * Math.min(1, depth / TAB_DRAG_EDGE),
      );
      const before = scroller.scrollLeft;
      scroller.scrollLeft = before + (intoRight > 0 ? step : -step);
      if (scroller.scrollLeft === before) return;
      track();
      frameRef.current = requestAnimationFrame(tick);
    };
    frameRef.current = requestAnimationFrame(tick);
  }, [track]);

  /** End the drag in place; the release that follows only swallows its click. */
  const cancel = useCallback(() => {
    const press = pressRef.current;
    if (!press) return;
    press.cancelled = true;
    stopScrolling();
    setDrag(null);
  }, [stopScrolling]);

  /** A left-button press on a workspace tab may become a drag. */
  const onPointerDown = useCallback(
    (
      event: React.PointerEvent<HTMLElement>,
      tab: WorkspaceTab,
      from: number,
    ) => {
      // Touch keeps its native sideways scroll; a second pointer is ignored.
      if (
        event.button !== 0 ||
        event.pointerType === "touch" ||
        pressRef.current
      ) {
        return;
      }
      // A fresh press clears any suppression a cancelled drag left behind.
      suppressClickRef.current = false;
      pressRef.current = {
        pointerId: event.pointerId,
        key: workspaceTabKey(tab),
        from,
        index: from,
        startX: event.clientX,
        clientX: event.clientX,
        keys,
        dragging: false,
        cancelled: false,
        grab: 0,
        layout: null,
      };
      // jsdom has no pointer capture; a browser keeps the move events on the
      // tab even when the pointer leaves the strip.
      event.currentTarget.setPointerCapture?.(event.pointerId);
    },
    [keys],
  );

  /** Past the threshold the press is a drag: measure once, then track. */
  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const press = pressRef.current;
      if (!press || press.pointerId !== event.pointerId || press.cancelled)
        return;
      press.clientX = event.clientX;
      if (!press.dragging) {
        if (Math.abs(event.clientX - press.startX) < TAB_DRAG_THRESHOLD) return;
        const scroller = scrollerRef.current;
        if (!scroller) return;
        const box = scroller.getBoundingClientRect();
        const groups = Array.from(
          scroller.querySelectorAll<HTMLElement>(
            '[data-testid="workspace-tab-group"]',
          ),
        );
        const rects = groups.map((group) => group.getBoundingClientRect());
        if (rects.length !== tabsRef.current.length || !rects[press.from])
          return;
        const lefts = rects.map(
          (rect) => rect.left - box.left + scroller.scrollLeft,
        );
        const widths = rects.map((rect) => rect.width);
        const gap =
          lefts.length > 1 ? Math.max(0, lefts[1] - lefts[0] - widths[0]) : 0;
        press.layout = { lefts, widths, shift: widths[press.from] + gap };
        press.grab = press.startX - rects[press.from].left;
        press.dragging = true;
      }
      track();
      scrollNearEdge();
    },
    [scrollNearEdge, track],
  );

  /** Release commits the landing place; a cancelled pointer only ends the press. */
  const endPress = useCallback(
    (event: React.PointerEvent<HTMLElement>, tab: WorkspaceTab) => {
      const press = pressRef.current;
      if (!press || press.pointerId !== event.pointerId) return;
      pressRef.current = null;
      stopScrolling();
      const target = event.currentTarget;
      if (target.hasPointerCapture?.(event.pointerId)) {
        target.releasePointerCapture(event.pointerId);
      }
      if (!press.dragging) return;
      // Only a completed press is followed by a click, dispatched before
      // the next task; a cancelled press produces none, so the suppression
      // disarms right after rather than swallowing a later keyboard
      // activation.
      if (event.type === "pointerup") {
        suppressClickRef.current = true;
        setTimeout(() => {
          suppressClickRef.current = false;
        }, 0);
      }
      setDrag(null);
      if (
        event.type === "pointerup" &&
        !press.cancelled &&
        press.keys === tabKeys(tabsRef.current) &&
        press.index !== press.from
      ) {
        moveTab(tab, press.index);
      }
    },
    [moveTab, stopScrolling],
  );

  // A capture lost without a release (the tab left the page) ends the press.
  const onLostPointerCapture = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const press = pressRef.current;
      if (!press || press.pointerId !== event.pointerId) return;
      pressRef.current = null;
      stopScrolling();
      setDrag(null);
    },
    [stopScrolling],
  );

  /** A click that ends a drag is the drag's, not a selection. */
  const consumeDragClick = useCallback(() => {
    if (!suppressClickRef.current) return false;
    suppressClickRef.current = false;
    return true;
  }, []);

  // Escape cancels a drag; the tabs return to where they were.
  const dragging = drag !== null;
  useEffect(() => {
    if (!dragging) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      cancel();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [cancel, dragging]);

  useEffect(() => stopScrolling, [stopScrolling]);

  // Keyboard reorder: the moved tab keeps focus once it is in its new place.
  const refocusRef = useRef<string | null>(null);
  const moveByKey = useCallback(
    (event: React.KeyboardEvent, tab: WorkspaceTab, index: number) => {
      if (
        !(event.ctrlKey || event.metaKey) ||
        !event.shiftKey ||
        event.altKey ||
        (event.key !== "ArrowLeft" && event.key !== "ArrowRight")
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const to = index + (event.key === "ArrowRight" ? 1 : -1);
      if (to < 0 || to >= tabsRef.current.length || pressRef.current) return;
      refocusRef.current = workspaceTabKey(tab);
      moveTab(tab, to);
    },
    [moveTab],
  );
  useLayoutEffect(() => {
    const key = refocusRef.current;
    if (!key) return;
    refocusRef.current = null;
    const group = Array.from(
      scrollerRef.current?.querySelectorAll<HTMLElement>(
        '[data-testid="workspace-tab-group"]',
      ) ?? [],
    ).find((candidate) => candidate.dataset.tabKey === key);
    group?.querySelector<HTMLElement>('[role="tab"]')?.focus();
  }, [tabs]);

  /** The sideways offset a tab is drawn at while a drag is under way. */
  const offsetOf = (index: number): number => {
    if (!drag) return 0;
    if (index === drag.from) return drag.offset;
    if (drag.from < index && index <= drag.index) return -drag.shift;
    if (drag.index <= index && index < drag.from) return drag.shift;
    return 0;
  };

  return {
    scrollerRef,
    drag,
    offsetOf,
    onPointerDown,
    onPointerMove,
    endPress,
    onLostPointerCapture,
    consumeDragClick,
    moveByKey,
  };
}

export const WorkspaceTabStrip: React.FC<WorkspaceTabStripProps> = ({
  trailing,
}) => {
  const {
    tabs,
    active,
    orgSlug,
    selectTab,
    closeTab,
    moveTab,
    openHome,
    openForkPicker,
  } = useWorkspaceTabs();
  const listedWorkspaces = useListedWorkspacesByName(orgSlug);
  const {
    scrollerRef,
    drag,
    offsetOf,
    onPointerDown,
    onPointerMove,
    endPress,
    onLostPointerCapture,
    consumeDragClick,
    moveByKey,
  } = useTabReorder(tabs, moveTab);

  const isActiveWorkspace = (tab: WorkspaceTab) =>
    active.kind === "workspace" && sameWorkspaceTab(active.tab, tab);

  // Roving focus along the tabs (WAI-ARIA tabs pattern, manual activation).
  // A modified arrow is a tab's own keyboard move, not focus movement.
  const onTablistKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) {
      return;
    }
    const list = event.currentTarget;
    const tabsInList = Array.from(
      list.querySelectorAll<HTMLElement>('[role="tab"]:not([aria-disabled])'),
    );
    const index = tabsInList.indexOf(document.activeElement as HTMLElement);
    if (index === -1) return;
    event.preventDefault();
    const step = event.key === "ArrowRight" ? 1 : -1;
    const next =
      tabsInList[(index + step + tabsInList.length) % tabsInList.length];
    next?.focus();
  }, []);

  return (
    <div
      data-testid="workspace-tab-bar"
      // The strip is the shell's chrome row now that there is no title bar:
      // themes that paint chrome (Aero Glass) paint it.
      data-surface="chrome"
      className="flex shrink-0 items-stretch border-b border-[var(--g5-stroke)] bg-[var(--g5-bg2)] pr-2"
      style={{ borderBottomWidth: "var(--g5-border-w)" }}
    >
      <div
        data-testid="workspace-tab-strip"
        role="tablist"
        aria-label="Open workspaces"
        className="flex min-w-0 flex-1 select-none items-end gap-0.5 pl-2 pt-1.5"
        onKeyDown={onTablistKeyDown}
      >
        <div
          className={classNames(
            TAB_GROUP,
            active.kind === "home" ? TAB_GROUP_ACTIVE : TAB_GROUP_IDLE,
          )}
        >
          <button
            type="button"
            role="tab"
            data-testid="workspace-tab-home"
            aria-selected={active.kind === "home"}
            aria-label="Home"
            title="Home"
            className={TAB_BUTTON}
            onClick={openHome}
          >
            <House size={15} />
          </button>
        </div>
        {/* Scrolls sideways when tabs overflow, Home staying put beside it;
            never vertically, so a theme's paint on a tab cannot turn one
            spare pixel into a scrollbar. */}
        <div
          ref={scrollerRef}
          data-testid="workspace-tab-scroller"
          className="flex min-w-0 flex-1 items-end gap-0.5 overflow-x-auto overflow-y-hidden pr-2"
        >
          {tabs.map((tab, index) => {
            const selected = isActiveWorkspace(tab);
            const key = workspaceTabKey(tab);
            const dragged = drag?.key === key;
            const offset = offsetOf(index);
            // The listed workspace behind the tab: its glyph and its name as
            // listed; until listed, the tab shows the name it was routed by.
            // A personal route is labelled and shows an avatar, not looked up.
            const listed = tab.personal
              ? undefined
              : listedWorkspaces.get(tab.name.toLowerCase());
            const name = listed?.name ?? workspaceTabLabel(tab);
            return (
              <div
                key={key}
                data-testid="workspace-tab-group"
                data-tab-key={key}
                data-dragging={dragged || undefined}
                className={classNames(
                  TAB_GROUP,
                  selected ? TAB_GROUP_ACTIVE : TAB_GROUP_IDLE,
                  dragged && TAB_GROUP_DRAGGED,
                  // An idle tab has no surface of its own; lifted over its
                  // neighbours it needs one.
                  dragged && !selected && "bg-[var(--g5-bg1)]",
                  // The tabs a drag passes glide aside; the dragged one
                  // follows the pointer. Without a drag nothing animates,
                  // so the committed order lands without a jump.
                  drag && !dragged && "transition-transform duration-150",
                )}
                style={
                  offset !== 0
                    ? { transform: `translateX(${offset}px)` }
                    : undefined
                }
              >
                <button
                  type="button"
                  role="tab"
                  data-testid="workspace-tab"
                  data-tab={workspaceTabPath(tab)}
                  aria-selected={selected}
                  aria-label={name}
                  aria-keyshortcuts="Control+Shift+ArrowLeft Control+Shift+ArrowRight Meta+Shift+ArrowLeft Meta+Shift+ArrowRight"
                  title={name}
                  className={classNames(TAB_BUTTON, "pr-1")}
                  onClick={() => {
                    if (consumeDragClick()) return;
                    selectTab(tab);
                  }}
                  onAuxClick={(event) => {
                    // Middle-click closes, as in a browser (mock story B1).
                    if (event.button === 1) {
                      event.preventDefault();
                      closeTab(tab);
                    }
                  }}
                  onKeyDown={(event) => moveByKey(event, tab, index)}
                  onPointerDown={(event) => onPointerDown(event, tab, index)}
                  onPointerMove={onPointerMove}
                  onPointerUp={(event) => endPress(event, tab)}
                  onPointerCancel={(event) => endPress(event, tab)}
                  onLostPointerCapture={onLostPointerCapture}
                >
                  <WorkspaceTabGlyph tab={tab} phase={listed?.phase} />
                  <span className="truncate">{name}</span>
                </button>
                <button
                  type="button"
                  data-testid="workspace-tab-close"
                  aria-label={`Close ${name}`}
                  title="Close tab"
                  className={classNames(
                    "mr-1.5 inline-flex h-[18px] w-[18px] shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--g5-text2)] hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
                    FOCUS_RING,
                  )}
                  onClick={() => closeTab(tab)}
                >
                  <X size={11} />
                </button>
              </div>
            );
          })}
          {active.kind === "fork" && (
            <div className={classNames(TAB_GROUP, TAB_GROUP_ACTIVE)}>
              <span
                role="tab"
                data-testid="workspace-tab-new"
                aria-selected
                aria-disabled
                className={classNames(TAB_BUTTON, "cursor-default")}
              >
                <Plus size={13} className="shrink-0 text-[var(--g5-text2)]" />
                <span className="truncate">New tab</span>
              </span>
            </div>
          )}
          <button
            type="button"
            data-testid="workspace-tab-add"
            aria-label="New tab"
            title="New tab"
            className={classNames(
              "mb-[3px] ml-1 inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--g5-text2)] transition-colors duration-150 hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
              FOCUS_RING,
            )}
            onClick={openForkPicker}
          >
            <Plus size={14} />
          </button>
        </div>
      </div>
      {trailing}
    </div>
  );
};
