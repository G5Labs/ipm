// =============================================================================
// Home — Workspaces tile (g5.ui.home-workspaces)
// =============================================================================
// Rendered by HomePage only under the `alignment` layout: the ONE Home list
// of the organization's workspaces, replacing both the lineage directory and
// Recent Workspaces. A table-shaped list with a visual header — Workspace,
// Approvals, Opened, Owner, actions — over a flat list: Mine (owned by the
// caller, the default) or All (other people's review copies omitted), a
// search box, and a sort — Recently opened by me (the per-browser open
// record, else the workspace's last update), Recently updated, Created,
// Name. Filter and sort live in the URL (`?workspaces=all`, `?sort=…`,
// defaults omitted); the page size is the person's `homeRowsPerTile` setting
// with a "Show N more" footer (g5.ui.home-paging).
//
// Each row: a chevron with a presence dot only while the workspace has
// running tasks (it discloses the recent tasks; nothing loops — the dot pulses
// once on a change and a polite status region names it), the name as the
// Open link with a subline (running tasks, the parent of a fork, open PRs,
// open questions), the workspace approval summary, the time the row is sorted
// by, the owner as in Running compute, and Delete (owner only; the same
// control invisible keeps the columns aligned) then Fork. A narrow tile folds
// the opened time first and the approvals next; the name, owner and actions
// stay at every width (ROW_GRID_CLASSES). The rows are a list whose cells
// each carry their own accessible name; the header is visual only, and the
// owner column, an avatar alone, carries no heading.
// =============================================================================

import { ChevronRight, GitFork, Trash2 } from "lucide-react";
import React, { useEffect, useMemo, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";

import { useRecentWorkspaceOpens } from "../../hooks/useRecentWorkspaceOpens";
import {
  formatActivityRelativeTime,
  formatActivityRelativeTimeExpanded,
} from "../../lib/activityTime";
import {
  compareByRecentOpen,
  openedAtById,
  workspaceOpenedAt,
} from "../../lib/recentWorkspaces";
import type { OrgMemberDTO, WorkspaceSummary } from "../../lib/types";
import { findOwner, ownerName } from "../../lib/workspaceOwner";
import { workspacePagePath } from "../../lib/workspacePagePath";
import { withoutPersonalWorkspaces } from "../../lib/workspaceKind";
import { workspaceMatchesSearch } from "../../lib/workspaceTree";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import { SearchField } from "../ui/SearchField";
import { Button, classNames } from "../ui/UI";
import { ReadOnlyBadge } from "../workspace/ReadOnlyBadge";
import { ApprovalGates } from "./ApprovalGates";
import {
  HomeFilterChip,
  HomeSectionHeader,
  HomeShowMore,
  HomeSortSelect,
  useHomePages,
} from "./HomeTileParts";
import {
  HomeWorkspaceTasksPanel,
  useHomeWorkspaceTasks,
} from "./HomeWorkspaceTasksPanel";
import {
  useHomeWorkspaceSignals,
  type HomeWorkspaceSignals,
} from "./useHomeWorkspaceSignals";

export type HomeWorkspacesFilter = "mine" | "all";
export type HomeWorkspacesSort = "opened" | "updated" | "created" | "name";

/** URL parameter names (defaults are omitted from the URL). */
export const HOME_WORKSPACES_FILTER_PARAM = "workspaces";
export const HOME_WORKSPACES_SORT_PARAM = "sort";

const FILTERS: Array<{ value: HomeWorkspacesFilter; label: string }> = [
  { value: "mine", label: "Mine" },
  { value: "all", label: "All" },
];

export const HOME_WORKSPACES_SORTS: Array<{
  value: HomeWorkspacesSort;
  label: string;
}> = [
  { value: "opened", label: "Recently opened" },
  { value: "updated", label: "Recently updated" },
  { value: "created", label: "Created" },
  { value: "name", label: "Name" },
];

/**
 * The URL omits the person's default (ui g5.ui.home-workspaces): Mine once
 * the onboarding tour is finished, All before.
 */
function readFilter(
  params: URLSearchParams,
  defaultFilter: HomeWorkspacesFilter,
): HomeWorkspacesFilter {
  const value = params.get(HOME_WORKSPACES_FILTER_PARAM);
  if (value === "all" || value === "mine") return value;
  return defaultFilter;
}

function readSort(params: URLSearchParams): HomeWorkspacesSort {
  const value = params.get(HOME_WORKSPACES_SORT_PARAM);
  return HOME_WORKSPACES_SORTS.some((option) => option.value === value)
    ? (value as HomeWorkspacesSort)
    : "opened";
}

/**
 * Filter, search and order the organization's list (pure, so the rule is
 * testable apart from the tile): Mine keeps the caller's workspaces, All
 * drops other people's review copies; personal workspaces are never listed
 * (g5.personal.home); the search matches name and branches; every sort breaks
 * ties on the name.
 */
export function selectHomeWorkspaces(
  workspaces: readonly WorkspaceSummary[],
  options: {
    callerUserId: string;
    filter: HomeWorkspacesFilter;
    sort: HomeWorkspacesSort;
    query: string;
    openedAt: ReadonlyMap<string, string>;
  },
): WorkspaceSummary[] {
  const normalizedQuery = options.query.trim().toLowerCase();
  const time = (value: string) => new Date(value).getTime() || 0;
  const byRecentOpen = compareByRecentOpen(options.openedAt);
  const compare = (left: WorkspaceSummary, right: WorkspaceSummary) => {
    switch (options.sort) {
      case "opened":
        return byRecentOpen(left, right);
      case "updated":
        return time(right.lastUpdated) - time(left.lastUpdated);
      case "created":
        return time(right.createdAt) - time(left.createdAt);
      default:
        return 0;
    }
  };
  return withoutPersonalWorkspaces(workspaces)
    .filter((workspace) =>
      options.filter === "mine"
        ? workspace.ownerUserId === options.callerUserId
        : !!workspace.agentReview ||
          workspace.isReview !== true ||
          workspace.ownerUserId === options.callerUserId,
    )
    .filter((workspace) => workspaceMatchesSearch(workspace, normalizedQuery))
    .sort(
      (left, right) =>
        compare(left, right) ||
        left.name.localeCompare(right.name, undefined, {
          sensitivity: "base",
        }),
    );
}

/**
 * Column template shared by the header and every workspace row, by the
 * tile's own width (a container query, not the viewport: the tile shares a
 * row with another tile from 1024px up). At every width the chevron, the
 * name, the owner's avatar (the name is its tooltip) and the actions; from
 * 28rem the approvals column; from 36rem the opened time. So a narrow tile
 * gives up the opened time first and the approvals next, never the owner or
 * the actions. The name column takes whatever is left and wraps.
 */
const ROW_GRID_CLASSES =
  "grid grid-cols-[20px_minmax(0,1fr)_24px_auto] items-center gap-x-2.5 @md:grid-cols-[20px_minmax(0,1fr)_104px_24px_auto] @xl:grid-cols-[20px_minmax(0,1fr)_104px_64px_24px_auto]";
/** Under 28rem the approvals are not shown. */
const APPROVALS_CELL_CLASSES = "hidden @md:block";
/** Under 36rem the opened time is not shown. */
const OPENED_CELL_CLASSES = "hidden @xl:inline";

export interface HomeWorkspacesTileProps {
  orgId: string;
  orgSlug: string;
  callerUserId: string;
  /** The filter an absent URL parameter means; `mine` unless given. */
  defaultFilter?: HomeWorkspacesFilter;
  workspaces: WorkspaceSummary[];
  loading: boolean;
  error: boolean;
  onRetry: () => void;
  /** Rows per page (UserSettings.homeRowsPerTile). */
  pageSize: number;
  /** The organization roster, for the owner's name and avatar. */
  members: OrgMemberDTO[];
  canRemoveWorkspace: (workspace: WorkspaceSummary) => boolean;
  onRequestRemove: (workspace: WorkspaceSummary) => void;
  /** Fork a row (cloud workflows only; omitted hides Fork). */
  onForkWorkspace?: (workspace: WorkspaceSummary) => void;
}

export function HomeWorkspacesTile({
  orgId,
  orgSlug,
  callerUserId,
  defaultFilter = "mine",
  workspaces: listedWorkspaces,
  loading,
  error,
  onRetry,
  pageSize,
  members,
  canRemoveWorkspace,
  onRequestRemove,
  onForkWorkspace,
}: HomeWorkspacesTileProps) {
  // Personal workspaces are not rows; the personal panel shows the caller's
  // own (g5.personal.home).
  const workspaces = useMemo(
    () => withoutPersonalWorkspaces(listedWorkspaces),
    [listedWorkspaces],
  );
  const [searchParams, setSearchParams] = useSearchParams();
  const filter = readFilter(searchParams, defaultFilter);
  const sort = readSort(searchParams);
  const [query, setQuery] = useState("");
  const paging = useHomePages(pageSize);
  const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  const setUrlState = (
    next: Partial<{ filter: HomeWorkspacesFilter; sort: HomeWorkspacesSort }>,
  ) => {
    setSearchParams(
      (params) => {
        const updated = new URLSearchParams(params);
        if (next.filter !== undefined) {
          if (next.filter === defaultFilter)
            updated.delete(HOME_WORKSPACES_FILTER_PARAM);
          else updated.set(HOME_WORKSPACES_FILTER_PARAM, next.filter);
        }
        if (next.sort !== undefined) {
          if (next.sort === "opened")
            updated.delete(HOME_WORKSPACES_SORT_PARAM);
          else updated.set(HOME_WORKSPACES_SORT_PARAM, next.sort);
        }
        return updated;
      },
      { replace: true },
    );
    paging.reset();
  };

  // The per-browser open record joins live against the list, so nothing
  // stored can name a workspace that no longer exists; another tab's open
  // re-sorts through the storage event.
  const recentOpens = useRecentWorkspaceOpens(callerUserId);
  const openedAt = useMemo(() => openedAtById(recentOpens), [recentOpens]);
  const selected = useMemo(
    () =>
      selectHomeWorkspaces(workspaces, {
        callerUserId,
        filter,
        sort,
        query,
        openedAt,
      }),
    [workspaces, callerUserId, filter, sort, query, openedAt],
  );
  const visible = selected.slice(0, paging.pages * pageSize);
  const remaining = Math.max(0, selected.length - visible.length);
  const nameById = useMemo(
    () =>
      new Map(workspaces.map((workspace) => [workspace.id, workspace.name])),
    [workspaces],
  );
  // Per-row reads for the visible page only (positive evidence, no page walk).
  const signalsByWorkspace = useHomeWorkspaceSignals(
    orgId,
    visible,
    callerUserId,
  );

  // Working-state changes are announced once, through one polite region,
  // for the rows on screen; every row's state is remembered so a row that
  // scrolls into the list later is not announced for an old change.
  const [announcement, setAnnouncement] = useState({ seq: 0, text: "" });
  const workingRef = useRef<Map<string, boolean> | null>(null);
  const visibleIds = useMemo(
    () => new Set(visible.map((workspace) => workspace.id)),
    [visible],
  );
  useEffect(() => {
    if (loading || error) return;
    const next = new Map(
      workspaces.map((workspace) => [
        workspace.id,
        workspace.concurrentTasks > 0,
      ]),
    );
    const previous = workingRef.current;
    workingRef.current = next;
    if (!previous) return;
    const changes: string[] = [];
    for (const workspace of workspaces) {
      if (!visibleIds.has(workspace.id)) continue;
      const before = previous.get(workspace.id);
      const now = next.get(workspace.id) ?? false;
      if (before === undefined || before === now) continue;
      changes.push(
        now
          ? `${workspace.name} started working`
          : `${workspace.name} finished working`,
      );
    }
    // A new node per announcement, so the same words twice still announce.
    if (changes.length > 0)
      setAnnouncement((current) => ({
        seq: current.seq + 1,
        text: changes.join(". "),
      }));
  }, [workspaces, loading, error, visibleIds]);

  const emptyCopy = query.trim()
    ? filter === "mine"
      ? `No workspace you own matches "${query.trim()}".`
      : `No workspace matches "${query.trim()}".`
    : filter === "mine"
      ? "No workspaces you own. Start one with New workspace."
      : "No workspaces yet.";

  return (
    <section data-testid="home-workspaces" className="min-w-0 space-y-2">
      <HomeSectionHeader
        title="Workspaces"
        count={loading || error ? undefined : selected.length}
        countLabel="workspaces listed"
        countTestId="home-workspaces-count"
        actions={
          <div className="flex w-full flex-wrap items-center gap-2 sm:ml-auto sm:w-auto sm:justify-end">
            <div
              role="group"
              aria-label="Filter workspaces"
              className="flex items-center gap-1"
            >
              {FILTERS.map((option) => (
                <HomeFilterChip
                  key={option.value}
                  testId={`home-workspaces-filter-${option.value}`}
                  selected={option.value === filter}
                  label={option.label}
                  onClick={() => setUrlState({ filter: option.value })}
                />
              ))}
            </div>
            <HomeSortSelect
              testId="home-workspaces-sort"
              label="Sort workspaces"
              value={sort}
              options={HOME_WORKSPACES_SORTS}
              onChange={(next) => setUrlState({ sort: next })}
            />
          </div>
        }
      />
      <SearchField
        testId="home-workspace-search"
        label="Search workspaces"
        placeholder="Search workspaces..."
        value={query}
        onChange={(value) => {
          setQuery(value);
          paging.reset();
        }}
      />
      <p
        role="status"
        aria-live="polite"
        data-testid="home-workspaces-live"
        className="sr-only"
      >
        {announcement.text && (
          <span key={announcement.seq}>{announcement.text}</span>
        )}
      </p>
      <div className="@container overflow-hidden rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
        <div
          aria-hidden="true"
          className={classNames(
            ROW_GRID_CLASSES,
            "border-b border-[var(--g5-stroke)] px-3 py-1.5 text-[10.5px] font-semibold uppercase tracking-[0.05em] text-[var(--g5-text2)]",
          )}
        >
          <span />
          <span>Workspace</span>
          <span className={APPROVALS_CELL_CLASSES}>Approvals</span>
          <span className={OPENED_CELL_CLASSES}>Opened</span>
          {/* The owner column is an avatar alone, too narrow for a heading;
              the avatar's tooltip and screen-reader text name it. */}
          <span />
          {/* The row's own controls, invisible, so the headings sit over
              their columns whatever width the actions take. */}
          <span className="invisible flex items-center justify-end gap-1">
            <Button kind="ghost" aria-hidden="true" tabIndex={-1} disabled>
              <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
            </Button>
            {onForkWorkspace && (
              <Button
                kind="secondary"
                aria-hidden="true"
                tabIndex={-1}
                disabled
              >
                <GitFork className="h-3.5 w-3.5" aria-hidden="true" />
                Fork
              </Button>
            )}
          </span>
        </div>
        {loading && (
          <ul
            role="status"
            aria-label="Loading workspaces"
            data-testid="home-workspaces-loading"
            className="divide-y divide-[var(--g5-stroke)]"
          >
            {Array.from({ length: 3 }, (_, index) => (
              <li
                key={index}
                aria-hidden="true"
                className={classNames(ROW_GRID_CLASSES, "px-3 py-3")}
              >
                <span />
                <span className="h-3.5 w-2/3 animate-pulse rounded bg-[var(--g5-bg2)] motion-reduce:animate-none" />
                <span className="hidden h-[18px] w-24 animate-pulse rounded bg-[var(--g5-bg2)] motion-reduce:animate-none @md:inline-block" />
                <span className="hidden h-3 w-10 animate-pulse rounded bg-[var(--g5-bg2)] motion-reduce:animate-none @xl:inline-block" />
                <span className="mx-auto h-[18px] w-[18px] animate-pulse rounded-full bg-[var(--g5-bg2)] motion-reduce:animate-none" />
                <span />
              </li>
            ))}
            <li className="sr-only">Loading workspaces…</li>
          </ul>
        )}
        {!loading && error && (
          <div
            role="alert"
            aria-label="Workspaces error"
            className="flex items-center justify-between gap-3 px-4 py-4"
          >
            <span className="text-sm text-[var(--g5-text1)]">
              Could not load this organization&rsquo;s workspaces.
            </span>
            <button
              type="button"
              data-testid="home-workspaces-retry"
              onClick={onRetry}
              className="rounded border border-[var(--g5-stroke)] px-2.5 py-1 text-sm text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
            >
              Retry
            </button>
          </div>
        )}
        {!loading && !error && visible.length === 0 && (
          <p
            data-testid="home-workspaces-empty"
            className="px-4 py-5 text-sm text-[var(--g5-text2)]"
          >
            {emptyCopy}
          </p>
        )}
        {!loading && !error && visible.length > 0 && (
          <ul
            aria-label="Workspaces"
            className="divide-y divide-[var(--g5-stroke)]"
          >
            {visible.map((workspace) => (
              <HomeWorkspaceRow
                key={workspace.id}
                orgSlug={orgSlug}
                workspace={workspace}
                parentName={
                  workspace.clonedFromId
                    ? (nameById.get(workspace.clonedFromId) ?? null)
                    : null
                }
                reads={
                  signalsByWorkspace[workspace.id] ?? {
                    approvals: { status: "loading" },
                    signals: [],
                  }
                }
                openedAt={workspaceOpenedAt(workspace, openedAt)}
                members={members}
                expanded={expandedIds.has(workspace.id)}
                onToggle={() =>
                  setExpandedIds((current) => {
                    const next = new Set(current);
                    if (next.has(workspace.id)) next.delete(workspace.id);
                    else next.add(workspace.id);
                    return next;
                  })
                }
                canRemove={canRemoveWorkspace(workspace)}
                onRequestRemove={onRequestRemove}
                onForkWorkspace={onForkWorkspace}
              />
            ))}
          </ul>
        )}
        {!loading && !error && (
          <HomeShowMore
            remaining={remaining}
            pageSize={pageSize}
            noun="workspace"
            testId="home-workspaces-show-more"
            onShowMore={paging.showMore}
          />
        )}
      </div>
    </section>
  );
}

interface HomeWorkspaceRowProps {
  orgSlug: string;
  workspace: WorkspaceSummary;
  parentName: string | null;
  reads: HomeWorkspaceSignals;
  openedAt: { at: string; opened: boolean };
  members: OrgMemberDTO[];
  expanded: boolean;
  onToggle: () => void;
  canRemove: boolean;
  onRequestRemove: (workspace: WorkspaceSummary) => void;
  onForkWorkspace?: (workspace: WorkspaceSummary) => void;
}

function HomeWorkspaceRow({
  orgSlug,
  workspace,
  parentName,
  reads,
  openedAt,
  members,
  expanded,
  onToggle,
  canRemove,
  onRequestRemove,
  onForkWorkspace,
}: HomeWorkspaceRowProps) {
  const working = workspace.concurrentTasks > 0;
  const tasksRegionId = `home-workspace-tasks-${workspace.id}`;
  // A row reads its tasks only while it works and is disclosed.
  const workspaceTasks = useHomeWorkspaceTasks(
    workspace.id,
    working && expanded && !workspace.agentReview,
  );
  const owner = findOwner(members, workspace.ownerUserId ?? null);
  const ownerLabel = ownerName(members, workspace.ownerUserId ?? null);
  const taskNoun = workspace.concurrentTasks === 1 ? "task" : "tasks";
  const subline: React.ReactNode[] = [];
  if (working) {
    subline.push(
      <span key="working" className="font-medium text-[var(--g5-text1)]">
        {workspace.concurrentTasks} running {taskNoun}
      </span>,
    );
  }
  if (parentName) {
    subline.push(
      <span key="parent">
        forked from{" "}
        <Link
          to={workspacePagePath(orgSlug, parentName)}
          className="text-[var(--g5-text1)] underline decoration-dotted hover:text-[var(--g5-text0)]"
        >
          {parentName}
        </Link>
      </span>,
    );
  }
  for (const signal of reads.signals)
    subline.push(<span key={signal}>{signal}</span>);
  const openedLabel = `${openedAt.opened ? "Last opened by you" : "Updated"} ${formatActivityRelativeTimeExpanded(openedAt.at)}`;

  return (
    <li
      data-testid={`home-workspace-${workspace.id}`}
      data-working={working || undefined}
      className="px-3 py-2.5"
    >
      <div className={ROW_GRID_CLASSES}>
        {working && !workspace.agentReview ? (
          <button
            type="button"
            data-testid="home-workspace-disclosure"
            aria-expanded={expanded}
            aria-controls={tasksRegionId}
            aria-label={`Working, ${workspace.concurrentTasks} running ${taskNoun}. ${expanded ? "Collapse" : "Expand"} ${workspace.name}`}
            title={`${expanded ? "Collapse" : "Expand"} ${workspace.name}`}
            onClick={onToggle}
            className="relative grid h-5 w-5 place-items-center rounded-full text-[var(--g5-text2)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          >
            <ChevronRight
              aria-hidden="true"
              size={14}
              className={classNames(
                "transition-transform motion-reduce:transition-none",
                expanded && "rotate-90",
              )}
            />
            <span
              aria-hidden="true"
              data-testid="home-workspace-presence"
              className="home-presence-dot absolute right-px top-px h-[7px] w-[7px] rounded-full bg-[var(--g5-info)] shadow-[0_0_0_2px_var(--g5-bg1)]"
            />
          </button>
        ) : (
          <span aria-hidden="true" className="h-5 w-5" />
        )}
        <div className="min-w-0">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
            <Link
              to={
                workspacePagePath(
                  orgSlug,
                  workspace.agentReview?.sourceWorkspaceName ?? workspace.name,
                ) + (workspace.agentReview ? "?view=approvals" : "")
              }
              data-testid="home-workspace-open"
              aria-label={`Open ${workspace.name}`}
              className="min-w-0 break-words text-sm font-semibold text-[var(--g5-text0)] no-underline hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
            >
              {workspace.name}
            </Link>
            {workspace.agentReview && (
              <span className="text-xs">Agent review</span>
            )}
            {workspace.readOnly === true && !workspace.agentReview && (
              <ReadOnlyBadge compact />
            )}
          </div>
          {subline.length > 0 && (
            <p
              data-testid="home-workspace-subline"
              className="mt-0.5 truncate text-[11.5px] text-[var(--g5-text2)]"
            >
              {subline.map((part, index) => (
                <React.Fragment key={index}>
                  {index > 0 && <span aria-hidden="true"> · </span>}
                  {part}
                </React.Fragment>
              ))}
            </p>
          )}
        </div>
        <span className={APPROVALS_CELL_CLASSES}>
          {!workspace.agentReview && (
            <ApprovalGates
              read={reads.approvals}
              workspaceName={workspace.name}
            />
          )}
        </span>
        <time
          dateTime={openedAt.at}
          title={openedLabel}
          aria-label={openedLabel}
          className={classNames(
            OPENED_CELL_CLASSES,
            "whitespace-nowrap text-[11px] tabular-nums text-[var(--g5-text2)]",
          )}
        >
          {formatActivityRelativeTime(openedAt.at)}
        </time>
        <span
          data-testid="home-workspace-owner"
          className="flex items-center justify-center"
          title={ownerLabel}
        >
          <IdentityAvatar
            identity={owner?.email ?? workspace.ownerUserId ?? "?"}
            avatarUrl={owner?.avatarUrl}
            members={members}
            size={18}
            title={ownerLabel}
          />
          <span className="sr-only">Owner: {ownerLabel}</span>
        </span>
        <span className="flex items-center justify-end gap-1">
          {canRemove && !workspace.agentReview ? (
            <Button
              kind="ghost"
              data-testid="home-workspace-remove"
              aria-label={`Remove ${workspace.name}`}
              title={`Remove ${workspace.name}`}
              onClick={() => onRequestRemove(workspace)}
            >
              <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
            </Button>
          ) : (
            // The same control, invisible, so Fork, Owner and Opened line up
            // down the list whatever a row may do.
            <Button
              kind="ghost"
              aria-hidden="true"
              tabIndex={-1}
              disabled
              data-testid="home-workspace-remove-slot"
              className="invisible"
            >
              <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
            </Button>
          )}
          {onForkWorkspace && !workspace.agentReview && (
            <Button
              kind="secondary"
              data-testid="home-workspace-fork"
              aria-label={`Fork ${workspace.name}`}
              onClick={() => onForkWorkspace(workspace)}
            >
              <GitFork className="h-3.5 w-3.5" aria-hidden="true" />
              Fork
            </Button>
          )}
        </span>
      </div>
      {working && expanded && (
        <HomeWorkspaceTasksPanel
          orgSlug={orgSlug}
          workspaceId={workspace.id}
          workspaceName={workspace.name}
          regionId={tasksRegionId}
          tasks={workspaceTasks}
          className="ml-[30px] mt-2"
        />
      )}
    </li>
  );
}
