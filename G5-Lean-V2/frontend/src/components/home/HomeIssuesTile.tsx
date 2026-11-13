// =============================================================================
// Home — Issues and tasks tile (workbench Phase 6)
// =============================================================================
// Rendered by HomePage only under the `alignment` layout (g5.workbench.gate).
// A triage slice, not a backlog: ONE search of the organization's open issues
// (`GET /orgs/:orgId/issues/search`, one request per configured domain,
// kept 5 minutes across Home visits) — the eight most recently updated. A
// compact search field above the list sends typed terms to that same search
// (GitHub's issue syntax, after a typing pause or on Enter; the termless
// slice stays cached, so clearing restores it at once). The filters All /
// Assigned to me / Unassigned and the label chips narrow the loaded slice on
// the client (a lower bound, never a second server query); "Assigned to me"
// matches the caller's connected GitHub login on GitHub issues (the account
// the server's own `assignee=me` uses) and the caller on native G5 issues,
// which need no GitHub connection. Refresh re-reads explicitly. Start
// workspace opens the fork-with-issue dialog, Link existing posts a workspace
// link (`POST /workspaces/:id/issue-links`) with the Workspace Issues panel's
// primary/related conflicts, and Create opens the Create issue dialog for the
// organization's creation target (g5.workbench.issues.create): visible but
// disabled with the reason while creation is impossible, and starting empty
// whatever the tile's filters.
// =============================================================================

import {
  CircleDot,
  Link2,
  LoaderCircle,
  Plus,
  RefreshCw,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useLocation } from "react-router-dom";

import {
  useExternalIssueSearch,
  useIssueCreateTarget,
  useIssueDomains,
  useLinkWorkspaceIssue,
} from "../../hooks/useExternalIssues";
import { useGithubConnection } from "../../hooks/useGithubConnection";
import { useAuth } from "../../hooks/useAuth";
import {
  describeIssueError,
  describeIssueRequestError,
  linkConflictOf,
} from "../../lib/externalIssueErrors";
import { githubConnectionPath } from "../../lib/githubConnection";
import { createTargetBlockedReason } from "../../lib/issueCommentErrors";
import {
  isNativeIssue,
  issueSettingsPath,
  nativeIssueHref,
} from "../../lib/issuePage";
import {
  ISSUE_SEARCH_DEBOUNCE_MS,
  isUnfinishedQuery,
} from "../../lib/issueSearchQuery";
import {
  formatActivityRelativeTime,
  formatActivityRelativeTimeExpanded,
} from "../../lib/activityTime";
import type {
  CreatedIssueDTO,
  ExternalIssueDomainErrorDTO,
  ExternalIssueSummaryDTO,
  WorkspaceIssueLinkRole,
  WorkspaceSummary,
} from "../../lib/types";
import {
  IssueErrorNotice,
  IssueLabels,
} from "../external-issues/ExternalIssueParts";
import { issueUserLabel } from "../../lib/nativeIssues";
import { CreateIssueDialog } from "../external-issues/CreateIssueDialog";
import { IssueLinkConflictNotice } from "../external-issues/IssueLinkConflictNotice";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import { SearchField } from "../ui/SearchField";
import { Button, classNames } from "../ui/UI";
import {
  HomeFilterChip,
  HomeSectionHeader,
  HomeShowMore,
  HOME_ROW_ACTION_CLASSES,
  useHomePages,
} from "./HomeTileParts";
import {
  HOME_ISSUES_TILE_LIMIT,
  filterByAssignee,
  labelChipsWithSelection,
  tileRows,
  type HomeIssueAssigneeFilter,
} from "./homeTileLogic";

/** The open slice outlives tab switches and Home revisits within a session. */
export const HOME_ISSUES_STALE_MS = 5 * 60_000;

const ASSIGNEE_FILTERS: Array<{
  value: HomeIssueAssigneeFilter;
  label: string;
}> = [
  { value: "me", label: "Assigned to me" },
  { value: "any", label: "All" },
  { value: "none", label: "Unassigned" },
];

const EMPTY_COPY: Record<HomeIssueAssigneeFilter, string> = {
  any: "No open issues",
  me: "No open issues are assigned to you",
  none: "No open issues are unassigned",
};

const FILTER_CHIP_CLASSES =
  "inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

export interface HomeIssuesTileProps {
  orgId: string;
  orgSlug: string;
  /** Organization Admin: may configure issue domains. */
  canConfigureDomains: boolean;
  /** Fork-with-issue dialog; omitted where workspaces cannot be created. */
  onStartWorkspace?: (issue: ExternalIssueSummaryDTO) => void;
  /** Workspaces the caller owns (Recent Workspaces order), for Link existing. */
  linkTargets: WorkspaceSummary[];
  /**
   * Open a full issue search under the tile's filter; omitted (the alignment
   * Home has no full search) hides the View all footer and the tile pages its
   * loaded slice instead.
   */
  onViewAll?: (assignee: HomeIssueAssigneeFilter, labels: string[]) => void;
  /** Rows per page (UserSettings.homeRowsPerTile); the loaded slice caps it. */
  pageSize?: number;
  /**
   * An issue was created from the tile's Create dialog; `fork` when the
   * person chose Create & Fork Workspace.
   */
  onIssueCreated: (issue: CreatedIssueDTO, options: { fork: boolean }) => void;
  /** Offer Create & Fork Workspace (workspaces can be created here). */
  allowCreateAndFork?: boolean;
  /**
   * The filter the tile opens on (ui g5.ui.home-workspaces): Assigned to me
   * once the onboarding tour is finished, All before.
   */
  defaultAssignee?: HomeIssueAssigneeFilter;
}

interface LinkWorkspacePickerProps {
  issue: ExternalIssueSummaryDTO;
  orgSlug: string;
  targets: WorkspaceSummary[];
  onDone: () => void;
}

/**
 * Link existing: one owned workspace from the recent list, linked as the
 * issue's primary; the Workspace Issues panel's conflicts apply through the
 * shared notice (a workspace with a primary, or an issue another workspace
 * owns, may still be linked as related).
 */
function LinkWorkspacePicker({
  issue,
  orgSlug,
  targets,
  onDone,
}: LinkWorkspacePickerProps) {
  const [workspaceId, setWorkspaceId] = useState(targets[0]?.id ?? "");
  const linkIssue = useLinkWorkspaceIssue(workspaceId);
  const conflict = linkConflictOf(linkIssue.error);
  const linkError = linkIssue.error && !conflict ? linkIssue.error : null;
  const submit = async (role: WorkspaceIssueLinkRole) => {
    if (!workspaceId) return;
    try {
      await linkIssue.mutateAsync({ ref: issue.displayKey, role });
      onDone();
    } catch {
      // The conflict or error notice below explains the failure.
    }
  };
  const selectId = `home-issue-link-target-${issue.externalIssueId}`;
  return (
    <div
      data-testid="home-issue-link-picker"
      className="mt-2 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 text-xs"
    >
      {targets.length === 0 ? (
        <p className="text-[var(--g5-text2)]">
          You own no workspace to link this issue to.
        </p>
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor={selectId} className="text-[var(--g5-text2)]">
            Link to
          </label>
          <select
            id={selectId}
            data-testid="home-issue-link-target"
            value={workspaceId}
            onChange={(event) => {
              setWorkspaceId(event.target.value);
              if (linkIssue.error) linkIssue.reset();
            }}
            className="h-7 min-w-0 flex-1 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 text-xs text-[var(--g5-text0)]"
          >
            {targets.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </select>
          <Button
            kind="primary"
            type="button"
            data-testid="home-issue-link-submit"
            disabled={linkIssue.isLoading || !workspaceId}
            onClick={() => void submit("primary")}
          >
            {linkIssue.isLoading ? (
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Link2 aria-hidden="true" className="h-3.5 w-3.5" />
            )}
            Link issue
          </Button>
        </div>
      )}
      {conflict && (
        <IssueLinkConflictNotice
          conflict={conflict}
          orgSlug={orgSlug}
          pending={linkIssue.isLoading}
          testIdPrefix="home-issue"
          className="mt-2"
          onLinkAsRelated={() => void submit("related")}
        />
      )}
      {linkError && (
        <div className="mt-2">
          <IssueErrorNotice
            compact
            testId="home-issue-link-error"
            presentation={describeIssueRequestError(linkError)}
          />
        </div>
      )}
    </div>
  );
}

interface IssueRowProps {
  issue: ExternalIssueSummaryDTO;
  orgSlug: string;
  linkTargets: WorkspaceSummary[];
  linking: boolean;
  onToggleLink: () => void;
  onStartWorkspace?: (issue: ExternalIssueSummaryDTO) => void;
}

function IssueRow({
  issue,
  orgSlug,
  linkTargets,
  linking,
  onToggleLink,
  onStartWorkspace,
}: IssueRowProps) {
  const primary = issue.primaryWorkspace;
  const assignee = issue.assignees[0];
  return (
    <li
      data-testid={`home-issue-${issue.externalIssueId}`}
      className="px-3 py-2.5"
    >
      <div className="flex min-w-0 items-start gap-2">
        <CircleDot
          aria-hidden="true"
          className="mt-0.5 h-4 w-4 flex-none text-[var(--g5-ok)]"
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <span className="flex-none font-mono text-xs text-[var(--g5-text2)]">
              {issue.displayKey}
            </span>
            {isNativeIssue(issue) ? (
              <Link
                to={nativeIssueHref(issue.canonicalUrl)}
                data-testid="home-issue-title"
                className="min-w-0 truncate text-sm font-semibold text-[var(--g5-text0)] no-underline hover:underline"
              >
                {issue.title}
              </Link>
            ) : (
              <a
                href={issue.canonicalUrl}
                target="_blank"
                rel="noreferrer"
                data-testid="home-issue-title"
                className="min-w-0 truncate text-sm font-semibold text-[var(--g5-text0)] no-underline hover:underline"
              >
                {issue.title}
              </a>
            )}
          </div>
          <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5">
            <IssueLabels labels={issue.labels} />
            {primary?.visible === true && (
              <Link
                to={`/${orgSlug}/${encodeURIComponent(primary.name)}`}
                data-testid="home-issue-workspace-chip"
                title={`Open workspace ${primary.name}`}
                className="inline-flex max-w-[12rem] items-center gap-1 truncate rounded-full border border-[var(--g5-stroke)] px-2 py-0.5 text-[11px] text-[var(--g5-text1)] no-underline hover:bg-[var(--g5-bg2)]"
              >
                {primary.name}
              </Link>
            )}
            {primary?.visible === false && (
              <span
                data-testid="home-issue-workspace-hidden"
                className="text-[11px] text-[var(--g5-text2)]"
              >
                Owned by another workspace
              </span>
            )}
          </div>
        </div>
        {assignee ? (
          <span
            data-testid="home-issue-assignee"
            className="flex flex-none items-center"
            title={issueUserLabel(assignee) ?? undefined}
          >
            <span className="sr-only">
              Assigned to {issueUserLabel(assignee)}
            </span>
            <IdentityAvatar
              identity={assignee.login}
              avatarUrl={assignee.avatarUrl}
              size={18}
            />
            {issue.assignees.length > 1 && (
              <span className="ml-1 text-[11px] text-[var(--g5-text2)]">
                +{issue.assignees.length - 1}
              </span>
            )}
          </span>
        ) : (
          <span
            data-testid="home-issue-assignee"
            className="flex-none text-[11px] text-[var(--g5-text2)]"
          >
            unassigned
          </span>
        )}
        <time
          dateTime={issue.updatedAt}
          aria-label={`Updated ${formatActivityRelativeTimeExpanded(issue.updatedAt)}`}
          className="flex-none whitespace-nowrap text-[11px] text-[var(--g5-text2)]"
        >
          <span className="sr-only">
            Updated {formatActivityRelativeTimeExpanded(issue.updatedAt)}
          </span>
          <span aria-hidden="true">
            {formatActivityRelativeTime(issue.updatedAt)}
          </span>
        </time>
        {primary?.visible === true && (
          <Link
            to={`/${orgSlug}/${encodeURIComponent(primary.name)}`}
            data-testid="home-issue-open"
            aria-label={`Open workspace ${primary.name}`}
            className={HOME_ROW_ACTION_CLASSES}
          >
            Open
          </Link>
        )}
        {primary === null && onStartWorkspace && (
          <button
            type="button"
            data-testid="home-issue-start-workspace"
            aria-label={`Start a workspace for ${issue.displayKey}`}
            onClick={() => onStartWorkspace(issue)}
            className={HOME_ROW_ACTION_CLASSES}
          >
            Start
          </button>
        )}
        {primary === null && (
          <button
            type="button"
            data-testid="home-issue-link-existing"
            aria-label={`Link ${issue.displayKey} to an existing workspace`}
            aria-expanded={linking}
            onClick={onToggleLink}
            className={HOME_ROW_ACTION_CLASSES}
          >
            {linking ? "Cancel" : "Link"}
          </button>
        )}
      </div>
      {linking && (
        <LinkWorkspacePicker
          issue={issue}
          orgSlug={orgSlug}
          targets={linkTargets}
          onDone={onToggleLink}
        />
      )}
    </li>
  );
}

function DomainErrorNotice({
  domainError,
  onRetry,
}: {
  domainError: ExternalIssueDomainErrorDTO;
  onRetry?: () => void;
}) {
  const presentation = describeIssueError(
    domainError.issueError,
    domainError.message,
  );
  return (
    <IssueErrorNotice
      compact
      testId={`home-issue-domain-error-${domainError.domainId}`}
      presentation={{
        title: `${domainError.displayKey}: ${presentation.title}`,
        detail: presentation.detail,
      }}
      onRetry={onRetry}
    />
  );
}

export function HomeIssuesTile({
  orgId,
  orgSlug,
  canConfigureDomains,
  onStartWorkspace,
  linkTargets,
  onViewAll,
  onIssueCreated,
  allowCreateAndFork = false,
  pageSize = HOME_ISSUES_TILE_LIMIT,
  defaultAssignee = "me",
}: HomeIssuesTileProps) {
  const location = useLocation();
  const [creating, setCreating] = useState(false);
  const createTarget = useIssueCreateTarget(orgId);
  const createBlockedReason = createTargetBlockedReason(createTarget);
  // The caller's own issues first (Home defaults to the caller's stuff);
  // All and Unassigned are a press away.
  const [assignee, setAssignee] =
    useState<HomeIssueAssigneeFilter>(defaultAssignee);
  const [selectedLabels, setSelectedLabels] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [linkingIssueId, setLinkingIssueId] = useState<string | null>(null);
  // The search field's text and the terms last sent: a typing pause sends
  // finished text (`isUnfinishedQuery`), Enter sends it at once, clearing
  // returns to the termless slice at once.
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");

  const domains = useIssueDomains(orgId);
  const domainCount = domains.status === "ok" ? domains.data.domains.length : 0;
  // "Assigned to me" needs a GitHub account only where every domain is a
  // GitHub repository; native issues match the signed-in person.
  const hasNativeDomain =
    domains.status === "ok" &&
    domains.data.domains.some((domain) => domain.provider === "g5");
  const { user } = useAuth();
  // The one search: every open issue across the domains matching the terms,
  // only once a domain exists (no request without a tracker); filters narrow
  // it on the client.
  const search = useExternalIssueSearch(
    orgId,
    query,
    "open",
    "any",
    domainCount > 0,
    { staleTime: HOME_ISSUES_STALE_MS },
  );
  const { state } = search;
  const connection = useGithubConnection().status;
  const githubLogin = connection.data?.githubLogin ?? null;
  const connectionKnown = connection.isSuccess || connection.isError;
  const loaded = useMemo(
    () => (state.status === "ok" ? state.data.results : []),
    [state],
  );
  const narrowed = useMemo(
    () => filterByAssignee(loaded, assignee, githubLogin, user?.id ?? null),
    [loaded, assignee, githubLogin, user?.id],
  );
  const chips = useMemo(
    () => labelChipsWithSelection(narrowed, selectedLabels),
    [narrowed, selectedLabels],
  );
  const paging = useHomePages(pageSize);
  // New terms start on the first page, however they were sent.
  const searchTerms = (next: string) => {
    setQuery(next);
    paging.reset();
  };
  useEffect(() => {
    if (isUnfinishedQuery(text)) return;
    const next = text.trim();
    if (next === query) return;
    const timer = window.setTimeout(
      () => searchTerms(next),
      ISSUE_SEARCH_DEBOUNCE_MS,
    );
    return () => window.clearTimeout(timer);
    // `searchTerms` is a per-render closure over stable setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, text]);
  // The filtered slice in full; the page shows the first pages × pageSize.
  const matching = useMemo(
    () => tileRows(narrowed, selectedLabels, Number.POSITIVE_INFINITY),
    [narrowed, selectedLabels],
  );
  const rows = useMemo(
    () => matching.slice(0, paging.pages * pageSize),
    [matching, paging.pages, pageSize],
  );
  const remaining = Math.max(0, matching.length - rows.length);
  const total = state.status === "ok" ? narrowed.length : undefined;
  const totalLabel =
    state.status === "ok" && state.data.truncated ? `${total}+` : `${total}`;
  // "Assigned to me" needs the caller's connected GitHub account; without
  // one the slice cannot be narrowed, so the connect hint takes its place.
  const connectionRequired =
    assignee === "me" && connectionKnown && !githubLogin && !hasNativeDomain;
  const domainErrors = state.status === "ok" ? state.data.domainErrors : [];
  // Domain failures ride inside a successful response: every repository
  // failing is "unavailable", some failing is a partial slice, none failing
  // with no rows is genuinely empty.
  const everyDomainFailed =
    state.status === "ok" &&
    domainErrors.length > 0 &&
    domainErrors.length >= state.data.domainCount &&
    state.data.results.length === 0;

  const changeText = (next: string) => {
    setText(next);
    if (next.trim() === "" && query !== "") searchTerms("");
  };
  // Enter searches the current text at once; text already searched (say,
  // after a failure) is searched again.
  const searchNow = () => {
    const next = text.trim();
    if (next !== query) searchTerms(next);
    else if (domainCount > 0) search.refetch();
  };

  const toggleLabel = (name: string) => {
    setSelectedLabels((current) => {
      const next = new Set(current);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
    paging.reset();
  };

  const emptyCopy = `${
    selectedLabels.size > 0
      ? "No open issues carry every selected label"
      : query !== ""
        ? `No open issues match “${query}”`
        : EMPTY_COPY[assignee]
  }${domainErrors.length > 0 ? " in the repositories that could be searched." : "."}`;

  return (
    <section data-testid="home-issues-tile" className="min-w-0 space-y-2">
      <HomeSectionHeader
        title="Issues and tasks"
        count={total}
        countLabel="open issues in the loaded slice"
        countTestId="home-issues-count"
        actions={
          // The search lives in the toolbar band above the list, so the
          // filters, Create and Refresh fit beside the title on one line.
          <div className="flex flex-wrap items-center gap-1 sm:ml-auto sm:justify-end">
            <div
              role="group"
              aria-label="Filter issues"
              className="flex flex-wrap items-center gap-1"
            >
              {ASSIGNEE_FILTERS.map((option) => (
                <HomeFilterChip
                  key={option.value}
                  testId={`home-issues-filter-${option.value}`}
                  selected={option.value === assignee}
                  label={option.label}
                  onClick={() => {
                    setAssignee(option.value);
                    paging.reset();
                  }}
                />
              ))}
            </div>
            <button
              type="button"
              data-testid="home-issues-create"
              disabled={createBlockedReason !== null}
              title={createBlockedReason ?? "Create an issue"}
              onClick={() => setCreating(true)}
              className={classNames(
                FILTER_CHIP_CLASSES,
                "border-[var(--g5-stroke)] bg-transparent font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent",
              )}
            >
              <Plus size={12} aria-hidden="true" />
              Create
            </button>
            {domainCount > 0 && (
              <button
                type="button"
                data-testid="home-issues-refresh"
                aria-label="Refresh open issues"
                title="Refresh open issues (one search per issue domain)"
                disabled={search.isFetching}
                onClick={search.refetch}
                className="inline-flex h-6 w-6 items-center justify-center rounded-full text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:cursor-wait disabled:opacity-60"
              >
                <RefreshCw
                  size={12}
                  aria-hidden="true"
                  className={search.isFetching ? "animate-spin" : undefined}
                />
              </button>
            )}
          </div>
        }
      />
      <div className="overflow-hidden rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
        {domainCount > 0 && (
          <div
            data-testid="home-issues-toolbar"
            // The chips take the remaining width and wrap among themselves;
            // the search keeps its width at the right end of the first row.
            className="flex items-start gap-2 border-b border-[var(--g5-stroke)] px-3 py-2"
          >
            {chips.length > 0 && (
              <div
                role="group"
                aria-label="Filter by label"
                data-testid="home-issues-label-chips"
                className="flex min-w-0 flex-1 flex-wrap items-center gap-1"
              >
                {chips.map((name) => {
                  const selected = selectedLabels.has(name);
                  return (
                    <button
                      key={name}
                      type="button"
                      data-testid={`home-issues-label-${name}`}
                      aria-pressed={selected}
                      onClick={() => toggleLabel(name)}
                      className={classNames(
                        FILTER_CHIP_CLASSES,
                        selected
                          ? "border-[var(--g5-info)]/40 bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)] font-semibold text-[var(--g5-info)]"
                          : "border-[var(--g5-stroke)] bg-transparent font-medium text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
                      )}
                    >
                      {name}
                    </button>
                  );
                })}
                {selectedLabels.size > 0 && (
                  <button
                    type="button"
                    data-testid="home-issues-label-clear"
                    onClick={() => {
                      setSelectedLabels(new Set());
                      paging.reset();
                    }}
                    className="inline-flex h-6 items-center gap-1 rounded-full px-2 text-[11px] text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
                  >
                    <X size={12} aria-hidden="true" />
                    Clear
                  </button>
                )}
              </div>
            )}
            <SearchField
              compact
              testId="home-issues-search"
              label="Search open issues"
              placeholder="Search"
              value={text}
              onChange={changeText}
              onKeyDown={(event) => {
                // Enter confirms an IME composition before it searches.
                if (event.key !== "Enter" || event.nativeEvent.isComposing) {
                  return;
                }
                event.preventDefault();
                searchNow();
              }}
              className="ml-auto w-32 flex-none"
            />
          </div>
        )}

        {domains.status === "loading" && (
          <p role="status" className="px-3 py-4 text-sm text-[var(--g5-text2)]">
            Loading issue domains…
          </p>
        )}
        {domains.status === "error" && (
          <div className="p-3">
            <IssueErrorNotice
              testId="home-issues-domains-error"
              presentation={describeIssueRequestError(domains.error)}
              onRetry={domains.refetch}
            />
          </div>
        )}
        {domains.status === "ok" && domainCount === 0 && (
          <div
            data-testid="home-issues-no-domains"
            className="px-3 py-5 text-sm text-[var(--g5-text1)]"
          >
            <p>No issue tracker is configured for this organization.</p>
            {canConfigureDomains ? (
              <p className="mt-1 text-xs text-[var(--g5-text2)]">
                Use G5 issues or add a GitHub repository in the organization's{" "}
                <Link
                  to={issueSettingsPath(orgSlug)}
                  data-testid="home-issues-configure-domain"
                  className="text-[var(--g5-info)] underline"
                >
                  Issues settings
                </Link>
                .
              </p>
            ) : (
              <p className="mt-1 text-xs text-[var(--g5-text2)]">
                An organization admin can set one up in the organization's
                Issues settings.
              </p>
            )}
          </div>
        )}

        {domainCount > 0 && state.status === "loading" && (
          <p
            role="status"
            data-testid="home-issues-loading"
            className="px-3 py-4 text-sm text-[var(--g5-text2)]"
          >
            Loading open issues…
          </p>
        )}
        {domainCount > 0 && state.status === "error" && (
          <div className="p-3">
            <IssueErrorNotice
              testId="home-issues-error"
              presentation={describeIssueRequestError(state.error)}
              onRetry={search.refetch}
            />
          </div>
        )}
        {domainCount > 0 && state.status === "ok" && everyDomainFailed && (
          <div
            data-testid="home-issues-unavailable"
            aria-label="Repositories that could not be searched"
            className="space-y-2 p-3"
          >
            {domainErrors.map((domainError, index) => (
              <DomainErrorNotice
                key={domainError.domainId}
                domainError={domainError}
                onRetry={
                  index === domainErrors.length - 1 ? search.refetch : undefined
                }
              />
            ))}
          </div>
        )}
        {domainCount > 0 && state.status === "ok" && !everyDomainFailed && (
          <>
            {domainErrors.length > 0 && (
              <ul
                data-testid="home-issues-domain-errors"
                aria-label="Repositories that could not be searched"
                className="space-y-2 border-b border-[var(--g5-stroke)] p-3"
              >
                {domainErrors.map((domainError) => (
                  <li key={domainError.domainId}>
                    <DomainErrorNotice
                      domainError={domainError}
                      onRetry={search.refetch}
                    />
                  </li>
                ))}
              </ul>
            )}
            {connectionRequired ? (
              <div
                data-testid="home-issues-connection-required"
                className="flex flex-wrap items-center gap-3 px-3 py-4 text-sm text-[var(--g5-text1)]"
              >
                <p className="min-w-0 flex-1">
                  Connect your GitHub account to see issues assigned to you.
                </p>
                <Link
                  data-testid="home-issues-connect"
                  to={githubConnectionPath(
                    `${location.pathname}${location.search}`,
                  )}
                  className={HOME_ROW_ACTION_CLASSES}
                >
                  Connect GitHub
                </Link>
              </div>
            ) : assignee === "me" && !connectionKnown && !hasNativeDomain ? (
              <p
                role="status"
                data-testid="home-issues-loading"
                className="px-3 py-4 text-sm text-[var(--g5-text2)]"
              >
                Checking your GitHub account…
              </p>
            ) : rows.length === 0 ? (
              <p
                data-testid="home-issues-empty"
                className="px-3 py-4 text-sm text-[var(--g5-text2)]"
              >
                {emptyCopy}
              </p>
            ) : (
              <ul
                data-testid="home-issues-rows"
                aria-label="Open issues"
                className="divide-y divide-[var(--g5-stroke)]"
              >
                {rows.map((issue) => (
                  <IssueRow
                    key={`${issue.trackerId}:${issue.externalIssueId}`}
                    issue={issue}
                    orgSlug={orgSlug}
                    linkTargets={linkTargets}
                    linking={linkingIssueId === issue.externalIssueId}
                    onToggleLink={() =>
                      setLinkingIssueId((current) =>
                        current === issue.externalIssueId
                          ? null
                          : issue.externalIssueId,
                      )
                    }
                    onStartWorkspace={onStartWorkspace}
                  />
                ))}
              </ul>
            )}
            {onViewAll ? (
              <button
                type="button"
                data-testid="home-issues-view-all"
                onClick={() => onViewAll(assignee, [...selectedLabels])}
                className="w-full border-t border-[var(--g5-stroke)] px-3 py-2 text-left text-sm text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)]"
              >
                View all {totalLabel} issues →
              </button>
            ) : (
              <HomeShowMore
                remaining={remaining}
                pageSize={pageSize}
                noun="issue"
                testId="home-issues-show-more"
                onShowMore={paging.showMore}
              />
            )}
          </>
        )}
      </div>
      <CreateIssueDialog
        orgId={orgId}
        open={creating}
        onClose={() => setCreating(false)}
        allowFork={allowCreateAndFork}
        onCreated={(issue, options) => {
          setCreating(false);
          onIssueCreated(issue, options);
        }}
      />
    </section>
  );
}
