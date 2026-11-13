// =============================================================================
// G5 Client — organization Home
// =============================================================================
// Organization Home at /:org/home. Home workspace and attention reads wait for
// the route organization to be synchronized into the active JWT. The
// Workspaces tile lists the organization (ui g5.ui.home-workspaces); Base's
// re-verification line sits above the tiles while the organization's
// verification switch is on (ui g5.ui.home-reverification). The
// operator organization's Home is the platform console instead
// (g5.organization.platform-operator.home).
// =============================================================================

import {
  forkLandingQuery,
  forkLandingState,
  type ForkLanding,
} from "../lib/workspaceLanding";
import { issuePagePath } from "../lib/issuePage";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Plus } from "lucide-react";
import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  Link,
  Navigate,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";

import { AppShell } from "../components/layout/Layout";
import { Button } from "../components/ui/UI";
import { ShellControls } from "../components/layout/ShellControls";
import { UserSettingsPanel } from "../components/settings/UserSettingsPanel";
import { CreateWorkspaceDialog } from "../components/workspace/CreateWorkspaceDialog";
import { CloneDialog } from "../components/workspace/CloneDialog";
import { RemoveWorkspaceDialog } from "../components/workspace/RemoveWorkspaceDialog";
import { NotificationToastStack } from "../components/notifications/NotificationToastStack";
import {
  canRemoveWorkspaceFromSwitcher,
  workspaceIdsWithLiveChildren,
} from "../components/workspace/workspaceSwitcherPolicy";
import { useAuth } from "../hooks/useAuth";
import { useClientToasts } from "../hooks/useClientToasts";
import {
  HOME_ATTENTION_DEFAULT_ORDER,
  homeAttentionQueryKey,
  homeWorkspacesRefetchInterval,
  useHomeAttention,
} from "../hooks/useHome";
import { useOrgMembers } from "../hooks/useOrg";
import { useInstanceInfo, useOrgScopedConfig } from "../hooks/useSettings";
import { useUserSettings } from "../hooks/useUserSettings";
import { useWorkspaceTabs } from "../hooks/useWorkspaceTabs";
import {
  useDeleteWorkspace,
  workspacesQueryOptions,
} from "../hooks/useWorkspace";
import { useRecentWorkspaceOpens } from "../hooks/useRecentWorkspaceOpens";
import {
  formatActivityRelativeTime,
  formatActivityRelativeTimeExpanded,
} from "../lib/activityTime";
import { compareByRecentOpen, openedAtById } from "../lib/recentWorkspaces";
import {
  applyDeploymentWorkspaceUiCapabilities,
  getWorkspaceUiCapabilities,
} from "../lib/capabilities";
import type {
  HomeAttentionKind,
  HomeAttentionItem,
  HomeAttentionOrder,
  OrgMemberDTO,
  WorkspaceSummary,
} from "../lib/types";
import { findOwner, ownerName } from "../lib/workspaceOwner";
import { IdentityAvatar } from "../components/shared/IdentityAvatar";
import { HomeComputeTile } from "../components/home/HomeComputeTile";
import { HomeIssuesTile } from "../components/home/HomeIssuesTile";
import { HomeReverificationStatus } from "../components/home/HomeReverificationStatus";
import { HomeStopMyWorkspaces } from "../components/home/HomeStopMyWorkspaces";
import { HomeWorkspacesTile } from "../components/home/HomeWorkspacesTile";
import { PersonalChatSidePanel } from "../components/personal/PersonalChatSidePanel";
import { PersonalHomeProvider } from "../components/personal/PersonalHomeContext";
import { PersonalPanel } from "../components/personal/PersonalPanel";
import { PlatformHome } from "../components/platform/PlatformHome";
import { withoutPersonalWorkspaces } from "../lib/workspaceKind";
import {
  HomeSectionHeader,
  HomeSortSelect,
  HOME_ROW_ACTION_CLASSES,
} from "../components/home/HomeTileParts";
import { useExternalIssuesFeature } from "../hooks/useExternalIssues";
import { useOnboardingTour } from "../components/onboarding/onboardingTourContext";
import { homeDefaultsToAll } from "../lib/onboarding";
import { isOrgVerificationActionEnabled } from "../lib/verificationActionFlags";

const BASE_WORKSPACE_UI_CAPABILITIES = getWorkspaceUiCapabilities();

function questionCopy(count: number): string {
  return count === 1
    ? "1 open question needs an answer"
    : `${count} open questions need an answer`;
}

function attentionReason(item: HomeAttentionItem): string {
  if (item.kind === "approval") return item.workspaceName;
  return questionCopy(item.openQuestionCount ?? 1);
}

function attentionHref(orgSlug: string, item: HomeAttentionItem): string {
  const workspace = `/${orgSlug}/${encodeURIComponent(item.workspaceName)}`;
  if (item.kind === "approval") return `${workspace}?view=approvals`;
  return `${workspace}/doc/${encodeURIComponent(item.sourceId)}`;
}

const ATTENTION_LABELS: Record<HomeAttentionItem["kind"], string> = {
  approval: "Approval",
  open_question: "Open question",
};

const ATTENTION_BADGE_CLASSES: Record<HomeAttentionItem["kind"], string> = {
  approval:
    "border-[var(--g5-warn)]/40 bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)] text-[var(--g5-warn)]",
  open_question:
    "border-[var(--g5-info)]/40 bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)] text-[var(--g5-info)]",
};

type AttentionFilter = "all" | HomeAttentionKind;

const ATTENTION_FILTERS: Array<{
  value: AttentionFilter;
  label: string;
}> = [
  { value: "all", label: "All" },
  { value: "open_question", label: "Open question" },
  { value: "approval", label: "Approval" },
];

const ATTENTION_FILTER_SELECTED_CLASSES: Record<AttentionFilter, string> = {
  all: "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
  approval: ATTENTION_BADGE_CLASSES.approval,
  open_question: ATTENTION_BADGE_CLASSES.open_question,
};

// The attention order lives in the URL with the default omitted, as the
// Workspaces sort does (g5.ui.home-attention-order-url-default).
const HOME_ATTENTION_ORDER_PARAM = "attentionSort";

const ATTENTION_ORDERS: Array<{ value: HomeAttentionOrder; label: string }> = [
  { value: "newest", label: "Newest" },
  { value: "oldest", label: "Oldest" },
];

function readAttentionOrder(params: URLSearchParams): HomeAttentionOrder {
  return params.get(HOME_ATTENTION_ORDER_PARAM) === "oldest"
    ? "oldest"
    : HOME_ATTENTION_DEFAULT_ORDER;
}

function attentionEmptyCopy(filter: AttentionFilter): string {
  switch (filter) {
    case "approval":
      return "No approvals need your attention.";
    case "open_question":
      return "No open questions need your attention.";
    default:
      return "Nothing needs your attention.";
  }
}

interface AttentionProps {
  orgSlug: string;
  items: HomeAttentionItem[];
  /** The organization roster, for each row's owner name and avatar. */
  members: OrgMemberDTO[];
  filter: AttentionFilter;
  order: HomeAttentionOrder;
  loading: boolean;
  error: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  onFilterChange: (filter: AttentionFilter) => void;
  onOrderChange: (order: HomeAttentionOrder) => void;
  onRetry: () => void;
  onShowMore: () => void;
}

function NeedsAttention({
  orgSlug,
  items,
  members,
  filter,
  order,
  loading,
  error,
  loadingMore,
  hasMore,
  onFilterChange,
  onOrderChange,
  onRetry,
  onShowMore,
}: AttentionProps) {
  return (
    <section data-testid="home-needs-attention" className="min-w-0 space-y-2">
      <HomeSectionHeader
        title="Needs Your Attention"
        testId="attention-heading-row"
        actions={
          <div className="flex w-full flex-wrap items-center gap-2 sm:ml-auto sm:w-auto sm:justify-end">
            <div
              role="group"
              aria-label="Filter attention items"
              className="flex flex-wrap items-center gap-1"
            >
              {ATTENTION_FILTERS.map((option) => {
                const selected = option.value === filter;
                return (
                  <button
                    key={option.value}
                    type="button"
                    data-testid={`home-attention-filter-${option.value}`}
                    aria-pressed={selected}
                    onClick={() => onFilterChange(option.value)}
                    className={`inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] ${
                      selected
                        ? `font-semibold ${ATTENTION_FILTER_SELECTED_CLASSES[option.value]}`
                        : "border-[var(--g5-stroke)] bg-transparent font-medium text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
                    }`}
                  >
                    {selected && <Check size={12} aria-hidden="true" />}
                    {option.label}
                  </button>
                );
              })}
            </div>
            <HomeSortSelect
              testId="home-attention-sort"
              label="Sort attention items"
              value={order}
              options={ATTENTION_ORDERS}
              onChange={onOrderChange}
            />
          </div>
        }
      />
      <div className="overflow-hidden rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
        {loading && (
          <p role="status" className="px-4 py-5 text-sm text-[var(--g5-text2)]">
            Loading attention items…
          </p>
        )}
        {error && items.length === 0 && (
          <div
            role="alert"
            aria-label="Needs Your Attention error"
            className="flex items-center justify-between gap-3 px-4 py-4"
          >
            <span className="text-sm text-[var(--g5-text1)]">
              Attention items are temporarily unavailable.
            </span>
            <button
              type="button"
              data-testid="home-attention-retry"
              onClick={onRetry}
              className="rounded border border-[var(--g5-stroke)] px-2.5 py-1 text-sm text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
            >
              Retry
            </button>
          </div>
        )}
        {!loading && !error && items.length === 0 && (
          <p className="px-4 py-5 text-sm text-[var(--g5-text2)]">
            {attentionEmptyCopy(filter)}
          </p>
        )}
        {items.length > 0 && (
          <ul className="divide-y divide-[var(--g5-stroke)]">
            {items.map((item) => {
              const reason = attentionReason(item);
              const primaryName = item.displayName || item.workspaceName;
              // The owning workspace's owner, the same roster lookup the
              // Workspaces tile makes; an unresolved owner falls back to the
              // deterministic initial (lib/workspaceOwner, IdentityAvatar).
              const owner = findOwner(members, item.ownerUserId ?? null);
              const ownerLabel = ownerName(members, item.ownerUserId ?? null);
              return (
                <li
                  key={item.id}
                  className="flex min-w-0 items-center gap-3 px-4 py-3"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 items-center gap-2">
                      <span
                        data-testid={`attention-kind-badge-${item.kind}`}
                        className={`flex-none rounded border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${ATTENTION_BADGE_CLASSES[item.kind]}`}
                      >
                        {ATTENTION_LABELS[item.kind]}
                      </span>
                      <Link
                        to={attentionHref(orgSlug, item)}
                        data-testid="home-attention-name"
                        className="min-w-0 truncate text-sm font-semibold text-[var(--g5-text0)] no-underline hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
                      >
                        {primaryName}
                      </Link>
                    </div>
                    <p className="mt-1 flex min-w-0 items-center gap-1 text-xs text-[var(--g5-text1)]">
                      <span
                        className={
                          item.kind === "approval" ? "truncate" : "flex-none"
                        }
                        title={
                          item.kind === "approval"
                            ? item.workspaceName
                            : undefined
                        }
                      >
                        {reason}
                      </span>
                      {item.kind !== "approval" && item.displayName && (
                        <>
                          <span aria-hidden="true">·</span>
                          <span className="flex-none">Workspace:</span>
                          <span className="truncate">{item.workspaceName}</span>
                        </>
                      )}
                    </p>
                  </div>
                  <span
                    data-testid="home-attention-owner"
                    className="flex flex-none items-center justify-center"
                    title={ownerLabel}
                  >
                    <IdentityAvatar
                      identity={owner?.email ?? item.ownerUserId ?? "?"}
                      avatarUrl={owner?.avatarUrl}
                      members={members}
                      size={18}
                      title={ownerLabel}
                    />
                    <span className="sr-only">Owner: {ownerLabel}</span>
                  </span>
                  <time
                    dateTime={item.actionableAt}
                    aria-label={`Attention updated ${formatActivityRelativeTimeExpanded(item.actionableAt)}`}
                    className="flex-none whitespace-nowrap text-right text-[11px] text-[var(--g5-text2)]"
                  >
                    <span className="sr-only">
                      Attention updated{" "}
                      {formatActivityRelativeTimeExpanded(item.actionableAt)}
                    </span>
                    <span aria-hidden="true">
                      {formatActivityRelativeTime(item.actionableAt)}
                    </span>
                  </time>
                  <Link
                    to={attentionHref(orgSlug, item)}
                    className={HOME_ROW_ACTION_CLASSES}
                  >
                    {item.kind === "approval" ? "Review" : "Answer"}
                  </Link>
                </li>
              );
            })}
          </ul>
        )}
        {error && items.length > 0 && (
          <div
            role="alert"
            className="flex items-center justify-between gap-3 border-t border-[var(--g5-stroke)] px-4 py-2"
          >
            <span className="text-xs text-[var(--g5-text2)]">
              More attention items could not be loaded.
            </span>
            <button
              type="button"
              data-testid="home-attention-load-more-retry"
              onClick={onRetry}
              className="text-xs underline"
            >
              Retry
            </button>
          </div>
        )}
        {hasMore && !error && (
          <button
            type="button"
            data-testid="home-attention-show-more"
            disabled={loadingMore}
            onClick={onShowMore}
            className="w-full border-t border-[var(--g5-stroke)] px-4 py-2 text-left text-sm text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] disabled:cursor-wait disabled:opacity-60"
          >
            {loadingMore ? "Loading more…" : "Show more attention items"}
          </button>
        )}
      </div>
    </section>
  );
}

export const HomePage: React.FC = () => {
  const { org: routeOrgSlug } = useParams<{ org: string }>();
  const { user, switchOrg, hasScope } = useAuth();
  const onboardingTour = useOnboardingTour();
  const instanceInfo = useInstanceInfo();
  const { settings: userSettings } = useUserSettings();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { toasts, pushToast, dismiss: dismissToast } = useClientToasts();
  // A removed workspace loses its tab in the strip (g5.workbench.tabs).
  const workspaceTabs = useWorkspaceTabs();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [createWorkspaceOpen, setCreateWorkspaceOpen] = useState(false);
  // Issue preselected when a workspace is started from an issue result.
  const [createWorkspaceIssueRef, setCreateWorkspaceIssueRef] = useState<
    string | null
  >(null);
  const [cloneSourceWorkspace, setCloneSourceWorkspace] =
    useState<WorkspaceSummary | null>(null);
  const [removeWorkspaceRequest, setRemoveWorkspaceRequest] =
    useState<WorkspaceSummary | null>(null);
  const [attentionFilterState, setAttentionFilterState] = useState<{
    orgId: string;
    value: AttentionFilter;
  }>({ orgId: "", value: "all" });
  const [orgSwitchError, setOrgSwitchError] = useState<string | null>(null);
  const attemptedOrgRef = useRef<string | null>(null);
  // Every Home tile pages by the person's setting (g5.workbench.home-workspaces).
  const pageSize = userSettings.homeRowsPerTile;

  const routeOrg = useMemo(
    () => user?.orgs.find((membership) => membership.orgSlug === routeOrgSlug),
    [routeOrgSlug, user],
  );
  const attentionFilter =
    routeOrg?.orgId === attentionFilterState.orgId
      ? attentionFilterState.value
      : "all";
  const attentionKind = attentionFilter === "all" ? null : attentionFilter;
  const [searchParams, setSearchParams] = useSearchParams();
  const attentionOrder = readAttentionOrder(searchParams);
  // The operator organization's Home is the platform console, which reads
  // none of the development tiles' data.
  const platformHome = routeOrg?.orgKind === "operator";
  const externalIssuesEnabled =
    useExternalIssuesFeature(platformHome ? undefined : routeOrg?.orgId)
      .status === "enabled";
  const capabilities = applyDeploymentWorkspaceUiCapabilities(
    BASE_WORKSPACE_UI_CAPABILITIES,
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  const routeOrgActive =
    !!routeOrg?.orgId && user?.activeOrgId === routeOrg.orgId;
  const developmentHomeActive = routeOrgActive && !platformHome;

  useEffect(() => {
    if (!routeOrg || routeOrgActive) {
      if (routeOrgActive) {
        attemptedOrgRef.current = null;
        setOrgSwitchError(null);
      }
      return;
    }
    if (attemptedOrgRef.current === routeOrg.orgId) return;

    attemptedOrgRef.current = routeOrg.orgId;
    setOrgSwitchError(null);
    void switchOrg(routeOrg.orgId).catch(() => {
      setOrgSwitchError(`Could not switch to ${routeOrg.orgName}.`);
    });
  }, [routeOrg, routeOrgActive, switchOrg]);

  const workspaceQuery = useQuery({
    ...workspacesQueryOptions(routeOrg?.orgId ?? ""),
    enabled: developmentHomeActive,
    // The Workspaces tile's working state follows this list: besides the
    // focus refetch, poll (quickly while any row runs tasks).
    refetchInterval: (query) => homeWorkspacesRefetchInterval(query.state.data),
  });
  // The Workspaces tile names owners from the organization roster, the same
  // read the ribbon's Owner control makes (lib/workspaceOwner).
  const orgMembers = useOrgMembers(
    developmentHomeActive ? (routeOrg?.orgId ?? "") : "",
  );
  // Base's re-verification status shows only while the organization's
  // verification switch is exactly on (ui g5.ui.home-reverification), read
  // as the workspace page reads it for Re-run Verification.
  const verificationOrgConfig = useOrgScopedConfig(
    developmentHomeActive ? (routeOrg?.orgId ?? "") : "",
  );
  const reverificationShown =
    developmentHomeActive &&
    verificationOrgConfig.status === "ok" &&
    isOrgVerificationActionEnabled(verificationOrgConfig.data);
  const deleteWorkspace = useDeleteWorkspace(
    removeWorkspaceRequest?.id ?? "",
    routeOrg?.orgId,
  );
  const workspaceIdsWithChildren = useMemo(
    () => workspaceIdsWithLiveChildren(workspaceQuery.data ?? []),
    [workspaceQuery.data],
  );
  const canRemoveWorkspace = (candidate: WorkspaceSummary) =>
    canRemoveWorkspaceFromSwitcher(candidate, {
      workspaceListLoaded: workspaceQuery.isSuccess,
      workspaceIdsWithChildren,
      routeOrgActive,
      hasWorkspaceWrite: hasScope("workspaces:write"),
      userId: user?.id,
    });

  const recentOpens = useRecentWorkspaceOpens(user?.id ?? "");
  const ownedWorkspaces = useMemo(() => {
    if (!user || !routeOrgActive) return [];
    // The personal workspace has its own panel (g5.personal.home).
    return withoutPersonalWorkspaces(workspaceQuery.data ?? [])
      .filter((workspace) => workspace.ownerUserId === user.id)
      .sort(compareByRecentOpen(openedAtById(recentOpens)));
  }, [user, workspaceQuery.data, routeOrgActive, recentOpens]);

  // The issues tile's Link picker: Base takes no issue links
  // (g5.issue.base-links), so an administrator's Base is left out.
  const issueLinkTargets = useMemo(
    () => ownedWorkspaces.filter((workspace) => !workspace.isBase),
    [ownedWorkspaces],
  );
  const attentionQuery = useHomeAttention(
    routeOrg?.orgId ?? "",
    attentionKind,
    developmentHomeActive,
    pageSize,
    attentionOrder,
  );
  const attentionItems =
    attentionQuery.data?.pages.flatMap((page) => page.items) ?? [];

  if (!user) return <Navigate to="/login" replace />;

  if (!routeOrg) {
    return (
      <AppShell>
        <main className="flex-1 bg-[var(--g5-bg0)] px-4 py-8">
          <p
            role="alert"
            className="mx-auto max-w-[1200px] text-sm text-[var(--g5-blocked)]"
          >
            You do not have access to this organization.
          </p>
        </main>
      </AppShell>
    );
  }

  if (!routeOrgActive) {
    return (
      <AppShell>
        <main className="flex-1 bg-[var(--g5-bg0)] px-4 py-8">
          <div className="mx-auto max-w-[1200px]">
            {orgSwitchError ? (
              <p role="alert" className="text-sm text-[var(--g5-blocked)]">
                {orgSwitchError} Try again from the organization switcher.
              </p>
            ) : (
              <p role="status" className="text-sm text-[var(--g5-text2)]">
                Switching to {routeOrg.orgName}…
              </p>
            )}
          </div>
        </main>
      </AppShell>
    );
  }

  if (platformHome) {
    return (
      <AppShell
        shellControls={
          <ShellControls onOpenAccount={() => setSettingsOpen(true)} />
        }
      >
        <div className="min-w-0 flex-1 overflow-y-auto bg-[var(--g5-bg0)]">
          <PlatformHome orgSlug={routeOrg.orgSlug} role={routeOrg.role} />
        </div>
        {settingsOpen && (
          <UserSettingsPanel
            open={settingsOpen}
            onClose={() => setSettingsOpen(false)}
            workspaceId=""
          />
        )}
      </AppShell>
    );
  }

  const openForkedWorkspace = (workspaceName: string, landing: ForkLanding) => {
    navigate(
      `/${routeOrg.orgSlug}/${encodeURIComponent(workspaceName)}${forkLandingQuery(landing)}`,
      { state: forkLandingState(landing) },
    );
  };
  const requestRemoveWorkspace = (workspace: WorkspaceSummary) => {
    if (!canRemoveWorkspace(workspace)) return;
    deleteWorkspace.reset();
    setRemoveWorkspaceRequest(workspace);
  };
  const closeRemoveWorkspaceDialog = () => {
    if (deleteWorkspace.isLoading) return;
    deleteWorkspace.reset();
    setRemoveWorkspaceRequest(null);
  };
  const confirmRemoveWorkspace = async () => {
    const target = removeWorkspaceRequest;
    if (!target) return;
    await deleteWorkspace.mutateAsync(undefined);
    queryClient.removeQueries({ queryKey: ["workspace", target.id] });
    setRemoveWorkspaceRequest(null);
    workspaceTabs.closeTab({ orgSlug: routeOrg.orgSlug, name: target.name });
  };

  const needsAttentionTile = (
    <NeedsAttention
      orgSlug={routeOrg.orgSlug}
      items={attentionItems}
      members={orgMembers.status === "ok" ? orgMembers.data : []}
      filter={attentionFilter}
      order={attentionOrder}
      loading={attentionQuery.isPending}
      error={attentionQuery.isError || attentionQuery.isFetchNextPageError}
      loadingMore={attentionQuery.isFetchingNextPage}
      hasMore={attentionQuery.hasNextPage}
      onFilterChange={(value) =>
        setAttentionFilterState({ orgId: routeOrg.orgId, value })
      }
      onOrderChange={(next) => {
        if (next === attentionOrder) return;
        // A choice always starts at the first page: drop any pages an
        // earlier visit to that order revealed (Show more) before the list
        // switches to it.
        queryClient.removeQueries({
          queryKey: homeAttentionQueryKey(
            routeOrg.orgId,
            attentionKind,
            pageSize,
            next,
          ),
          exact: true,
        });
        setSearchParams(
          (params) => {
            const updated = new URLSearchParams(params);
            if (next === HOME_ATTENTION_DEFAULT_ORDER)
              updated.delete(HOME_ATTENTION_ORDER_PARAM);
            else updated.set(HOME_ATTENTION_ORDER_PARAM, next);
            return updated;
          },
          { replace: true },
        );
      }}
      onRetry={() => {
        if (attentionQuery.isFetchNextPageError) {
          void attentionQuery.fetchNextPage();
        } else {
          void attentionQuery.refetch();
        }
      }}
      onShowMore={() => void attentionQuery.fetchNextPage()}
    />
  );
  const openNewWorkspace = () => {
    setCreateWorkspaceIssueRef(null);
    setCreateWorkspaceOpen(true);
  };
  const forkWorkspace = capabilities.cloudWorkspaceWorkflows
    ? setCloneSourceWorkspace
    : undefined;
  // The caller's personal workspace on Home: viewers own none.
  const personalHomeEnabled = routeOrg.role !== "viewer";
  // Until the onboarding tour is finished the tiles open on All (ui
  // g5.ui.home-workspaces); the tour seeds the scratchpad composer.
  const tileDefaultAll = homeDefaultsToAll(user);
  return (
    <AppShell
      // The strip carries the global controls (ui g5.ui.workbench-shell).
      shellControls={
        <ShellControls
          onOpenAccount={() => setSettingsOpen(true)}
          // Home is scoped to the person and the organization, so the
          // person's Settings page is one click away here too; the canonical
          // `/:org/settings` route lands on Base's settings tab
          // (ui g5.ui.settings-page).
          onOpenSettings={() => navigate(`/${routeOrgSlug}/settings`)}
        />
      }
    >
      <PersonalHomeScope
        enabled={personalHomeEnabled}
        orgId={routeOrg.orgId}
        orgSlug={routeOrg.orgSlug}
        userId={user.id}
        orgAdmin={routeOrg.role === "admin"}
      >
        <div className="flex min-h-0 flex-1">
          <div className="min-w-0 flex-1 overflow-y-auto bg-[var(--g5-bg0)]">
            <main className="mx-auto w-full max-w-[1400px] px-4 py-6 sm:px-6 lg:px-8">
              <div className="mb-6 flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
                    {routeOrg.orgName} Home
                  </h1>
                  <p className="mt-1 text-sm text-[var(--g5-text2)]">
                    Hi, {user.displayName.trim() || user.email}
                  </p>
                </div>
                {capabilities.cloudWorkspaceWorkflows && (
                  <Button
                    kind="primary"
                    data-testid="home-new-workspace"
                    onClick={openNewWorkspace}
                  >
                    <Plus size={14} aria-hidden="true" />
                    New workspace
                  </Button>
                )}
              </div>
              {/* The caller's personal workspace below the side panel's
                width: status and a composer without choosing a workspace
                (g5.personal.home). Viewers own no workspace. */}
              {routeOrg.role !== "viewer" && (
                <PersonalPanel
                  key={`personal-${routeOrg.orgId}`}
                  chatInSidePanel
                  className="mb-6"
                  orgId={routeOrg.orgId}
                  orgSlug={routeOrg.orgSlug}
                  userId={user.id}
                  orgAdmin={routeOrg.role === "admin"}
                />
              )}
              {developmentHomeActive && (
                <HomeReverificationStatus
                  key={`reverification-${routeOrg.orgId}`}
                  switchOn={reverificationShown}
                  orgId={routeOrg.orgId}
                  orgSlug={routeOrg.orgSlug}
                  members={orgMembers.status === "ok" ? orgMembers.data : []}
                  pageSize={pageSize}
                />
              )}
              <div
                data-testid="home-columns"
                className="grid grid-cols-1 gap-6 lg:grid-cols-2"
              >
                <HomeWorkspacesTile
                  key={`workspaces-${routeOrg.orgId}`}
                  orgId={routeOrg.orgId}
                  orgSlug={routeOrg.orgSlug}
                  callerUserId={user.id}
                  defaultFilter={tileDefaultAll ? "all" : "mine"}
                  workspaces={workspaceQuery.data ?? []}
                  loading={workspaceQuery.isPending}
                  error={workspaceQuery.isError && !workspaceQuery.data}
                  onRetry={() => void workspaceQuery.refetch()}
                  pageSize={pageSize}
                  members={orgMembers.status === "ok" ? orgMembers.data : []}
                  canRemoveWorkspace={canRemoveWorkspace}
                  onRequestRemove={requestRemoveWorkspace}
                  onForkWorkspace={forkWorkspace}
                />
                {needsAttentionTile}
                {externalIssuesEnabled && (
                  <HomeIssuesTile
                    key={`issues-${routeOrg.orgId}`}
                    orgId={routeOrg.orgId}
                    orgSlug={routeOrg.orgSlug}
                    canConfigureDomains={routeOrg.role === "admin"}
                    onStartWorkspace={
                      capabilities.cloudWorkspaceWorkflows
                        ? (issue) => {
                            setCreateWorkspaceIssueRef(issue.displayKey);
                            setCreateWorkspaceOpen(true);
                          }
                        : undefined
                    }
                    linkTargets={issueLinkTargets}
                    pageSize={pageSize}
                    defaultAssignee={tileDefaultAll ? "any" : "me"}
                    // Create opens the new issue's page; Create & Fork opens
                    // Start workspace's fork dialog with the issue preselected
                    // as primary (g5.workbench.issues.create).
                    allowCreateAndFork={capabilities.cloudWorkspaceWorkflows}
                    onIssueCreated={(issue, { fork }) => {
                      if (fork && capabilities.cloudWorkspaceWorkflows) {
                        setCreateWorkspaceIssueRef(issue.displayKey);
                        setCreateWorkspaceOpen(true);
                        return;
                      }
                      navigate(
                        issuePagePath(
                          routeOrg.orgSlug,
                          issue.repositoryId,
                          issue.number,
                        ),
                      );
                    }}
                  />
                )}
                <HomeComputeTile
                  key={`compute-${routeOrg.orgId}`}
                  orgId={routeOrg.orgId}
                  orgSlug={routeOrg.orgSlug}
                  callerUserId={user.id}
                  pageSize={pageSize}
                  defaultFilter={tileDefaultAll ? "all" : "mine"}
                  onNotify={pushToast}
                  headerAction={
                    // Stop by user for the caller (g5.lifecycle.stop.by-user)
                    // where their running VMs are listed; viewers own no
                    // workspace.
                    capabilities.cloudWorkspaceWorkflows &&
                    routeOrg.role !== "viewer" ? (
                      <HomeStopMyWorkspaces
                        key={`stop-mine-${routeOrg.orgId}`}
                        orgId={routeOrg.orgId}
                        orgName={routeOrg.orgName}
                        userId={user.id}
                        shutdownPending={
                          orgMembers.status === "ok" &&
                          orgMembers.data.some(
                            (member) =>
                              member.userId === user.id &&
                              member.shutdownPending,
                          )
                        }
                        onNotify={pushToast}
                      />
                    ) : undefined
                  }
                />
              </div>
            </main>
          </div>
          {/* The personal workspace's Chat, as in the workbench
            (g5.personal.home), with its status and notice; wide screens only,
            with the personal panel below that width. */}
          {personalHomeEnabled && (
            <PersonalChatSidePanel
              key={`personal-chat-${routeOrg.orgId}`}
              onNotify={pushToast}
              userId={user?.id}
              composerSeed={onboardingTour.composerSeed}
            />
          )}
        </div>
      </PersonalHomeScope>
      {settingsOpen && (
        <UserSettingsPanel
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          workspaceId=""
        />
      )}
      {capabilities.cloudWorkspaceWorkflows && createWorkspaceOpen && (
        <CreateWorkspaceDialog
          open
          onClose={() => setCreateWorkspaceOpen(false)}
          orgId={routeOrg.orgId}
          initialIssueRef={createWorkspaceIssueRef ?? undefined}
          onCreated={(workspaceName, landing) => {
            setCreateWorkspaceOpen(false);
            openForkedWorkspace(workspaceName, landing);
          }}
        />
      )}
      {capabilities.cloudWorkspaceWorkflows && cloneSourceWorkspace && (
        <CloneDialog
          source={cloneSourceWorkspace}
          orgId={routeOrg.orgId}
          onCloned={(workspace, landing) => {
            setCloneSourceWorkspace(null);
            openForkedWorkspace(workspace.name, landing);
          }}
          onClose={() => setCloneSourceWorkspace(null)}
        />
      )}
      {removeWorkspaceRequest && (
        <RemoveWorkspaceDialog
          workspace={removeWorkspaceRequest}
          isRemoving={deleteWorkspace.isLoading}
          errorMessage={deleteWorkspace.error?.message ?? null}
          onClose={closeRemoveWorkspaceDialog}
          onConfirm={confirmRemoveWorkspace}
        />
      )}
      {toasts.length > 0 && (
        <NotificationToastStack
          notifications={toasts}
          onDismiss={dismissToast}
          onAction={() => {}}
        />
      )}
    </AppShell>
  );
};

/** Shares one personal workspace state across Home when it shows one. */
function PersonalHomeScope({
  enabled,
  children,
  ...props
}: {
  enabled: boolean;
  orgId: string;
  orgSlug: string;
  userId: string;
  orgAdmin: boolean;
  children: React.ReactNode;
}) {
  if (!enabled) return <>{children}</>;
  return (
    <PersonalHomeProvider key={`personal-home-${props.orgId}`} {...props}>
      {children}
    </PersonalHomeProvider>
  );
}
