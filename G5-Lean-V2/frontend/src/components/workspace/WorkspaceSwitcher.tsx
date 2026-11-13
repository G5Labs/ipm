// =============================================================================
// G5 Workspace Page — WorkspaceSwitcher
// =============================================================================
// A searchable dropdown for switching workspaces within the top bar.
// Renders as a button showing the current workspace name. Clicking opens a
// dropdown panel with search input, workspace list, and "New Workspace" button.
//
// Consumes `useWorkspaces` hook for workspace list data.
// =============================================================================

import React, {
  useState,
  useRef,
  useEffect,
  useCallback,
  useMemo,
} from "react";
import {
  ChevronDown,
  GitFork,
  Info,
  Layers,
  Minus,
  Plus,
  Trash2,
} from "lucide-react";
import {
  getWorkspaceUiCapabilities,
  type WorkspaceUiCapabilities,
} from "../../lib/capabilities";
import { withoutPersonalWorkspaces } from "../../lib/workspaceKind";
import { workspaceMatchesSearch } from "../../lib/workspaceTree";
import { SearchField } from "../ui/SearchField";
import { classNames } from "../ui/UI";
import { ReadOnlyBadge } from "./ReadOnlyBadge";
import { ReviewBadge } from "./ReviewBadge";
import { useWorkspaces } from "../../hooks/useWorkspace";
import type { WorkspaceSummary } from "../../lib/types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface WorkspaceSwitcherProps {
  /** Currently active workspace */
  currentWorkspace: WorkspaceSummary | null;
  /** Trigger copy when no workspace is current */
  emptyLabel?: string;
  /** Organization ID */
  orgId: string;
  /** Organization slug (route segment) used to build each row's anchor href `/:orgSlug/:workspaceName` */
  orgSlug: string;
  /** Called when user selects a workspace */
  onSelectWorkspace: (workspaceId: string) => void;
  /** Called when user clicks "New Workspace" */
  onNewWorkspace: () => void;
  /** Called when user clicks the fork button for the current workspace */
  onForkCurrentWorkspace?: () => void;
  /** Called when user clicks the info button to open the workspace details page */
  onOpenWorkspaceDetails?: () => void;
  /** Called when user clicks the fork button on a specific dropdown row */
  onForkWorkspace?: (workspace: WorkspaceSummary) => void;
  /** Called when user clicks the remove button on a specific dropdown row */
  onRemoveWorkspace?: (workspace: WorkspaceSummary) => void;
  /** UI-only gate for whether a row can show the remove affordance */
  canRemoveWorkspace?: (workspace: WorkspaceSummary) => boolean;
  /** UI capability gates for cloud/project workflow affordances */
  capabilities?: WorkspaceUiCapabilities;
  /**
   * Whether the current workspace is a review workspace (a reviewer-owned
   * fork of the workspace under review); shows the Review badge on the
   * trigger so the tab reads differently from the real workspace.
   */
  currentWorkspaceIsReview?: boolean;
  /** False while the page has not learned if the current one is a copy. */
  reviewIdentityKnown?: boolean;
}

interface WorkspaceHierarchy {
  roots: WorkspaceSummary[];
  childrenByParentId: Map<string, WorkspaceSummary[]>;
  byId: Map<string, WorkspaceSummary>;
}

interface VisibleWorkspaceRow {
  workspace: WorkspaceSummary;
  depth: number;
  hasChildren: boolean;
  childCount: number;
  isExpanded: boolean;
}

function compareWorkspaces(left: WorkspaceSummary, right: WorkspaceSummary) {
  const updatedDelta =
    new Date(right.lastUpdated).getTime() -
    new Date(left.lastUpdated).getTime();
  if (updatedDelta !== 0) return updatedDelta;
  return left.name.localeCompare(right.name);
}

export function buildWorkspaceHierarchy(
  listed: WorkspaceSummary[],
  currentWorkspaceId: string | null = null,
): WorkspaceHierarchy {
  // A review copy is reached from its source's Approvals page (or a direct
  // link), never from the tree: it is the reviewer's scratch space, not a
  // workspace of the organization's own. The copy currently open is the
  // exception: it appears as its own row under its source, so the reviewer
  // sees where they are and can remove it from here
  // (`g5.workspace.ui.review-copy-row`). Personal workspaces have no
  // lineage and are reached from Home or `/{org}/me`, never from the tree
  // (g5.personal.home).
  const workspaces = withoutPersonalWorkspaces(listed).filter(
    (workspace) =>
      workspace.isReview !== true || workspace.id === currentWorkspaceId,
  );
  const byId = new Map(
    workspaces.map((workspace) => [workspace.id, workspace]),
  );
  const childrenByParentId = new Map<string, WorkspaceSummary[]>();
  const roots: WorkspaceSummary[] = [];

  for (const workspace of workspaces) {
    const parentId = workspace.clonedFromId;
    if (parentId && parentId !== workspace.id && byId.has(parentId)) {
      const children = childrenByParentId.get(parentId) ?? [];
      children.push(workspace);
      childrenByParentId.set(parentId, children);
    } else {
      roots.push(workspace);
    }
  }

  for (const children of childrenByParentId.values()) {
    children.sort(compareWorkspaces);
  }
  roots.sort(compareWorkspaces);

  const reachable = new Set<string>();
  const visit = (workspace: WorkspaceSummary) => {
    if (reachable.has(workspace.id)) return;
    reachable.add(workspace.id);
    for (const child of childrenByParentId.get(workspace.id) ?? []) {
      visit(child);
    }
  };
  roots.forEach(visit);

  for (const workspace of workspaces) {
    if (!reachable.has(workspace.id)) {
      roots.push(workspace);
    }
  }

  return { roots, childrenByParentId, byId };
}

// ---------------------------------------------------------------------------
// WorkspaceSwitcher Component
// ---------------------------------------------------------------------------

/**
 * Searchable workspace dropdown in the top bar.
 *
 * Shows current workspace name as trigger button. Dropdown contains:
 * - Search input for filtering workspaces by name
 * - Workspace list with a leading marker (dot / expand-collapse), name, and
 *   a per-workspace fork button
 * - "New Workspace" button at bottom
 * - "Connect Repository" action
 *
 * Closes on outside click or Escape key.
 * Keyboard navigation: arrow keys + Enter in dropdown.
 */
export const WorkspaceSwitcher: React.FC<WorkspaceSwitcherProps> = ({
  currentWorkspace,
  emptyLabel,
  orgId,
  orgSlug,
  onSelectWorkspace,
  onNewWorkspace,
  onForkCurrentWorkspace,
  onOpenWorkspaceDetails,
  onForkWorkspace,
  onRemoveWorkspace,
  canRemoveWorkspace,
  capabilities = getWorkspaceUiCapabilities(),
  currentWorkspaceIsReview = false,
  reviewIdentityKnown = true,
}) => {
  const showEmptyLabelIcon =
    currentWorkspace === null && emptyLabel !== undefined;
  const triggerLabel =
    currentWorkspace?.name ?? emptyLabel ?? "Select workspace";
  const showCloudWorkspaceWorkflows = capabilities.cloudWorkspaceWorkflows;
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(-1);
  // Subtrees are collapsed by default; ids present here are the rows the user
  // has explicitly expanded.
  const [expandedWorkspaceIds, setExpandedWorkspaceIds] = useState<Set<string>>(
    new Set(),
  );

  const containerRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Fetch workspace list
  const workspacesState = useWorkspaces(orgId);
  // The open review copy sits under its source, which starts collapsed: its
  // ancestors open once, when the list first names them, so the row is
  // visible at once (`g5.workspace.ui.review-copy-row`). A later manual
  // collapse is kept.
  const autoExpandedForRef = useRef<string | null>(null);
  useEffect(() => {
    if (
      !currentWorkspace ||
      currentWorkspace.isReview !== true ||
      autoExpandedForRef.current === currentWorkspace.id ||
      workspacesState.status !== "ok"
    ) {
      return;
    }
    const byId = new Map(workspacesState.data.map((ws) => [ws.id, ws]));
    const ancestors: string[] = [];
    let parentId = currentWorkspace.clonedFromId;
    while (parentId && byId.has(parentId) && !ancestors.includes(parentId)) {
      ancestors.push(parentId);
      parentId = byId.get(parentId)?.clonedFromId ?? null;
    }
    autoExpandedForRef.current = currentWorkspace.id;
    if (ancestors.length === 0) return;
    setExpandedWorkspaceIds((prev) => {
      const next = new Set(prev);
      for (const id of ancestors) next.add(id);
      return next;
    });
  }, [currentWorkspace, workspacesState]);
  const workspaceData =
    workspacesState.status === "ok" ? workspacesState.data : undefined;
  const workspaces = useMemo<WorkspaceSummary[]>(
    () => workspaceData ?? [],
    [workspaceData],
  );

  // Keep the active workspace visible during the load window: the org list is
  // complete once it resolves, but while `useWorkspaces` is still pending
  // `workspaces` is `[]`, and this prepend is the only reason the switcher can
  // highlight where the user already is before that lands. (The single
  // workspace-detail query usually resolves first.) It is not a guard against a
  // partial list — the endpoint is unpaginated and returns every workspace.
  const visibleWorkspaces = useMemo(() => {
    if (!currentWorkspace) {
      return workspaces;
    }
    return workspaces.some((ws) => ws.id === currentWorkspace.id)
      ? workspaces
      : [currentWorkspace, ...workspaces];
  }, [currentWorkspace, workspaces]);

  const hierarchy = useMemo(
    () =>
      buildWorkspaceHierarchy(visibleWorkspaces, currentWorkspace?.id ?? null),
    [currentWorkspace?.id, visibleWorkspaces],
  );

  const normalizedSearchQuery = searchQuery.trim().toLowerCase();

  const visibleRows = useMemo<VisibleWorkspaceRow[]>(() => {
    const rows: VisibleWorkspaceRow[] = [];
    const searchExpandedIds = new Set<string>();
    const matchCache = new Map<string, boolean>();

    const hasVisibleMatch = (
      workspace: WorkspaceSummary,
      path: Set<string>,
    ): boolean => {
      const cached = matchCache.get(workspace.id);
      if (cached !== undefined) return cached;
      if (path.has(workspace.id)) return false;

      const nextPath = new Set(path);
      nextPath.add(workspace.id);
      const ownMatch = workspaceMatchesSearch(workspace, normalizedSearchQuery);
      const childMatch = (
        hierarchy.childrenByParentId.get(workspace.id) ?? []
      ).some((child) => hasVisibleMatch(child, nextPath));

      if (normalizedSearchQuery && childMatch) {
        searchExpandedIds.add(workspace.id);
      }

      const matched = ownMatch || childMatch;
      matchCache.set(workspace.id, matched);
      return matched;
    };

    if (normalizedSearchQuery) {
      for (const root of hierarchy.roots) {
        hasVisibleMatch(root, new Set());
      }
    }

    const visited = new Set<string>();
    const visit = (workspace: WorkspaceSummary, depth: number) => {
      if (visited.has(workspace.id)) return;
      visited.add(workspace.id);

      const allChildren = hierarchy.childrenByParentId.get(workspace.id) ?? [];
      const children = normalizedSearchQuery
        ? allChildren.filter((child) => hasVisibleMatch(child, new Set()))
        : allChildren;
      const hasChildren = children.length > 0;
      // Collapsed by default: a row is expanded only when the user expanded it
      // or when an active search reveals a matching descendant.
      const isExpanded =
        hasChildren &&
        (searchExpandedIds.has(workspace.id) ||
          expandedWorkspaceIds.has(workspace.id));

      rows.push({
        workspace,
        depth,
        hasChildren,
        childCount: children.length,
        isExpanded,
      });

      if (isExpanded) {
        children.forEach((child) => visit(child, depth + 1));
      }
    };

    const roots = normalizedSearchQuery
      ? hierarchy.roots.filter((root) => hasVisibleMatch(root, new Set()))
      : hierarchy.roots;

    roots.forEach((root) => visit(root, 0));
    return rows;
  }, [expandedWorkspaceIds, hierarchy, normalizedSearchQuery]);

  const toggleWorkspaceExpanded = useCallback((workspaceId: string) => {
    setExpandedWorkspaceIds((prev) => {
      const next = new Set(prev);
      if (next.has(workspaceId)) {
        next.delete(workspaceId);
      } else {
        next.add(workspaceId);
      }
      return next;
    });
  }, []);

  // -----------------------------------------------------------------------
  // Open / close handlers
  // -----------------------------------------------------------------------

  const openDropdown = useCallback(() => {
    setIsOpen(true);
    setSearchQuery("");
    setHighlightedIndex(-1);
    // Focus search input after render
    setTimeout(() => searchInputRef.current?.focus(), 0);
  }, []);

  const closeDropdown = useCallback(() => {
    setIsOpen(false);
    setSearchQuery("");
    setHighlightedIndex(-1);
  }, []);

  const handleSelectWorkspace = useCallback(
    (workspaceId: string) => {
      onSelectWorkspace(workspaceId);
      closeDropdown();
    },
    [onSelectWorkspace, closeDropdown],
  );

  // -----------------------------------------------------------------------
  // Click outside handler
  // -----------------------------------------------------------------------

  useEffect(() => {
    if (!isOpen) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        closeDropdown();
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [isOpen, closeDropdown]);

  // -----------------------------------------------------------------------
  // Keyboard navigation
  // -----------------------------------------------------------------------

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!isOpen) return;

      // Row action buttons (per-workspace fork, expand/collapse) and the
      // trigger are real <button>s — let them handle their own Enter/Space/
      // Arrow activation instead of the listbox navigation/selection keys, so
      // keyboard users get the same result as a mouse click. Escape still
      // closes the dropdown from anywhere.
      const onActionButton = !!(e.target as HTMLElement | null)?.closest(
        "button",
      );

      switch (e.key) {
        case "Escape":
          e.preventDefault();
          closeDropdown();
          break;
        case "ArrowDown":
          if (onActionButton) break;
          e.preventDefault();
          setHighlightedIndex((prev) =>
            prev < visibleRows.length - 1 ? prev + 1 : prev,
          );
          break;
        case "ArrowUp":
          if (onActionButton) break;
          e.preventDefault();
          setHighlightedIndex((prev) => (prev > 0 ? prev - 1 : prev));
          break;
        case "Enter":
          if (onActionButton) break;
          e.preventDefault();
          if (highlightedIndex >= 0 && highlightedIndex < visibleRows.length) {
            handleSelectWorkspace(visibleRows[highlightedIndex].workspace.id);
          }
          break;
      }
    },
    [
      isOpen,
      closeDropdown,
      highlightedIndex,
      visibleRows,
      handleSelectWorkspace,
    ],
  );

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <div ref={containerRef} className="relative" onKeyDown={handleKeyDown}>
      <div className="flex items-center gap-1">
        {/* Trigger button */}
        <button
          type="button"
          data-testid="workspace-switcher-trigger"
          className={classNames(
            "inline-flex items-center gap-1.5",
            "px-2.5 py-1",
            "rounded-[var(--g5-radius-sm)]",
            "text-sm text-[var(--g5-text0)] font-medium",
            "hover:bg-[var(--g5-bg1)]",
            "transition-colors duration-150",
            "cursor-pointer",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            "max-w-[200px]",
          )}
          onClick={isOpen ? closeDropdown : openDropdown}
          aria-expanded={isOpen}
          aria-haspopup="listbox"
        >
          {showEmptyLabelIcon && (
            <Layers
              aria-hidden="true"
              data-testid="workspace-directory-icon"
              size={15}
              className="shrink-0 text-[var(--g5-text2)]"
            />
          )}
          <span className="truncate">{triggerLabel}</span>
          {currentWorkspaceIsReview && <ReviewBadge />}
          {currentWorkspace?.readOnly === true && <ReadOnlyBadge />}
          <ChevronDown
            size={14}
            className={classNames(
              "shrink-0 text-[var(--g5-text2)]",
              "transition-transform duration-150",
              isOpen && "rotate-180",
            )}
          />
        </button>

        {/* Workspace details info button — a read affordance, NOT cloud-gated
            (unlike the fork button); shown whenever there is a current
            workspace. Sits between the switcher trigger and the fork button. */}
        {currentWorkspace && onOpenWorkspaceDetails && (
          <button
            type="button"
            data-testid="workspace-info-button"
            className={classNames(
              "inline-flex items-center justify-center",
              "w-8 h-8",
              "rounded-[var(--g5-radius-sm)]",
              "text-[var(--g5-text1)]",
              "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
              "transition-colors duration-150",
              "cursor-pointer",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            )}
            onClick={() => {
              closeDropdown();
              onOpenWorkspaceDetails();
            }}
            title="Workspace details"
            aria-label="Workspace details"
          >
            <Info size={14} />
          </button>
        )}

        {showCloudWorkspaceWorkflows &&
          currentWorkspace &&
          onForkCurrentWorkspace && (
            <button
              type="button"
              data-testid="workspace-fork-current-button"
              className={classNames(
                "inline-flex items-center justify-center",
                "w-8 h-8",
                "rounded-[var(--g5-radius-sm)]",
                "text-[var(--g5-text1)]",
                "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
                "transition-colors duration-150",
                "cursor-pointer",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
              )}
              onClick={() => {
                closeDropdown();
                onForkCurrentWorkspace();
              }}
              title="Fork current project"
              aria-label="Fork current project"
            >
              <GitFork size={14} />
            </button>
          )}
      </div>

      {/* Dropdown panel */}
      {isOpen && (
        <div
          className={classNames(
            "absolute top-full left-0 mt-1 z-50",
            "w-[min(420px,calc(100vw-1.5rem))] max-h-[480px]",
            "bg-[var(--g5-bg1)]",
            "border border-[var(--g5-stroke)]",
            "rounded-[var(--g5-radius-md)]",
            "shadow-lg",
            "flex flex-col",
            "overflow-hidden",
          )}
          role="listbox"
        >
          {/* Search input */}
          <div className="px-3 py-2 border-b border-[var(--g5-stroke)]">
            <SearchField
              testId="workspace-search"
              clearTestId="workspace-search-clear"
              label="Search workspaces"
              placeholder="Search workspaces..."
              inputRef={searchInputRef}
              inputClassName="bg-[var(--g5-bg0)] text-[var(--g5-text1)]"
              value={searchQuery}
              onChange={(value) => {
                setSearchQuery(value);
                setHighlightedIndex(-1);
              }}
            />
          </div>

          {/* Workspace list */}
          <div className="flex-1 overflow-y-auto">
            {visibleRows.length === 0 ? (
              <div className="px-3 py-6 text-center text-sm text-[var(--g5-text2)]">
                {searchQuery
                  ? "No workspaces match your search."
                  : "No workspaces found."}
              </div>
            ) : (
              <ul className="py-1" role="listbox">
                {visibleRows.map((row, index) => {
                  const ws = row.workspace;
                  const isActive = ws.id === currentWorkspace?.id;
                  const isHighlighted = index === highlightedIndex;
                  // A review copy is never forked
                  // (`g5.governance.review-workspace.exclusions`); its row
                  // keeps Remove.
                  const showForkAction =
                    showCloudWorkspaceWorkflows &&
                    !!onForkWorkspace &&
                    ws.isReview !== true &&
                    !(
                      isActive &&
                      (currentWorkspaceIsReview || !reviewIdentityKnown)
                    );
                  const showRemoveAction =
                    !!onRemoveWorkspace &&
                    !ws.isBase &&
                    (canRemoveWorkspace?.(ws) ?? true);
                  const showRowActions = showForkAction || showRemoveAction;

                  return (
                    <li
                      key={ws.id}
                      role="option"
                      data-testid="workspace-switcher-row"
                      aria-selected={isActive}
                      className={classNames(
                        "pr-3 py-2 cursor-pointer",
                        "transition-colors duration-100",
                        isHighlighted && "bg-[var(--g5-bg2)]",
                        isActive && !isHighlighted && "bg-[var(--g5-bg0)]",
                        !isHighlighted &&
                          !isActive &&
                          "hover:bg-[var(--g5-bg2)]",
                      )}
                      style={{ paddingLeft: `${12 + row.depth * 18}px` }}
                      onClick={() => handleSelectWorkspace(ws.id)}
                      onMouseEnter={() => setHighlightedIndex(index)}
                    >
                      <div className="flex items-center gap-2">
                        {row.hasChildren ? (
                          <button
                            type="button"
                            data-testid="workspace-row-expand-toggle"
                            className={classNames(
                              "inline-flex items-center justify-center",
                              "w-5 h-5 rounded-[var(--g5-radius-sm)]",
                              "text-[var(--g5-text2)]",
                              "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
                              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
                              "shrink-0",
                            )}
                            onClick={(event) => {
                              event.stopPropagation();
                              toggleWorkspaceExpanded(ws.id);
                            }}
                            aria-label={`${row.isExpanded ? "Collapse" : "Expand"} ${ws.name}`}
                          >
                            {row.isExpanded ? (
                              <Minus size={13} strokeWidth={2.4} />
                            ) : (
                              <Plus size={13} strokeWidth={2.4} />
                            )}
                          </button>
                        ) : (
                          <span className="w-5 shrink-0 flex items-center justify-center">
                            <span
                              data-testid="workspace-leaf-marker"
                              className="w-1.5 h-1.5 rounded-full bg-[var(--g5-text2)]"
                              aria-hidden="true"
                            />
                          </span>
                        )}
                        {/* Real anchor to the workspace URL so the full name
                            (often truncated in the row) is reachable: the
                            browser status bar shows the URL on hover, the
                            `title` surfaces the full name as a tooltip, and
                            modifier-click opens the workspace in a new tab.
                            Plain left-click keeps SPA selection and is isolated
                            from the row so it does not double-fire. */}
                        <a
                          href={`/${orgSlug}/${encodeURIComponent(ws.name)}`}
                          data-testid="workspace-row-link"
                          title={ws.name}
                          className="text-sm font-medium text-[var(--g5-text0)] no-underline truncate flex-1"
                          onClick={(event) => {
                            // Let the browser handle new-tab / new-window intent.
                            if (
                              event.metaKey ||
                              event.ctrlKey ||
                              event.shiftKey ||
                              event.altKey
                            ) {
                              event.stopPropagation();
                              return;
                            }
                            // Plain click: SPA-select without a full page load,
                            // and stop the row's own select handler from also
                            // firing.
                            event.preventDefault();
                            event.stopPropagation();
                            handleSelectWorkspace(ws.id);
                          }}
                        >
                          {ws.name}
                        </a>
                        {ws.isReview === true && <ReviewBadge compact />}
                        {ws.readOnly === true && <ReadOnlyBadge compact />}

                        {showRowActions && (
                          <div className="flex shrink-0 items-center gap-1">
                            {showForkAction && (
                              <button
                                type="button"
                                data-testid="workspace-row-fork-button"
                                className={classNames(
                                  "inline-flex items-center justify-center",
                                  "w-6 h-6 shrink-0",
                                  "rounded-[var(--g5-radius-sm)]",
                                  "text-[var(--g5-text1)]",
                                  "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
                                  "transition-colors duration-150",
                                  "cursor-pointer",
                                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
                                )}
                                onClick={(event) => {
                                  // Isolate from the row's select/navigate handler.
                                  event.stopPropagation();
                                  closeDropdown();
                                  onForkWorkspace?.(ws);
                                }}
                                title={`Fork ${ws.name}`}
                                aria-label={`Fork ${ws.name}`}
                              >
                                <GitFork size={13} />
                              </button>
                            )}
                            {showRemoveAction && (
                              <button
                                type="button"
                                data-testid="workspace-row-remove-button"
                                className={classNames(
                                  "inline-flex items-center justify-center",
                                  "w-6 h-6 shrink-0",
                                  "rounded-[var(--g5-radius-sm)]",
                                  "text-[var(--g5-blocked)]",
                                  "hover:bg-[var(--g5-blocked)]/10 hover:text-[var(--g5-blocked)]",
                                  "transition-colors duration-150",
                                  "cursor-pointer",
                                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-blocked)]",
                                )}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  closeDropdown();
                                  onRemoveWorkspace?.(ws);
                                }}
                                title={`Remove ${ws.name}`}
                                aria-label={`Remove ${ws.name}`}
                              >
                                <Trash2 size={13} />
                              </button>
                            )}
                          </div>
                        )}
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          {/* Bottom actions */}
          {showCloudWorkspaceWorkflows && (
            <div className="border-t border-[var(--g5-stroke)] px-3 py-2 flex items-center gap-2">
              <button
                type="button"
                data-testid="workspace-new-button"
                className={classNames(
                  "inline-flex items-center gap-1.5",
                  "px-3 py-1.5",
                  "rounded-[var(--g5-radius-sm)]",
                  "text-sm text-[var(--g5-info)]",
                  "hover:bg-[var(--g5-bg2)]",
                  "transition-colors duration-150",
                  "cursor-pointer",
                  "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
                )}
                onClick={() => {
                  onNewWorkspace();
                  closeDropdown();
                }}
              >
                <Plus size={14} />
                New Workspace
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
