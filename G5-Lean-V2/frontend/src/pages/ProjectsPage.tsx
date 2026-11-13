// =============================================================================
// G5 Client — Projects Page
// =============================================================================
// Landing page showing workspaces across all org memberships. Uses
// WorkspaceTable internally. Provides search and navigation
// to workspace.
// =============================================================================

import { forkLandingQuery, forkLandingState } from "../lib/workspaceLanding";
import { useQueries } from "@tanstack/react-query";
import { GitBranch, Layers, Search } from "lucide-react";
import React, { useCallback, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { AppShell, TopBar } from "../components/layout/Layout";
import { CloneDialog } from "../components/workspace/CloneDialog";
import { CreateWorkspaceDialog } from "../components/workspace/CreateWorkspaceDialog";
import { WorkspaceTable } from "../components/workspace/WorkspaceTable";
import { useAuth } from "../hooks/useAuth";
import { useSwitchOrgThen } from "../hooks/useSwitchOrgThen";
import { workspacePagePath } from "../lib/workspacePagePath";
import { useInstanceInfo } from "../hooks/useSettings";
import { workspacesQueryOptions } from "../hooks/useWorkspace";
import {
  applyDeploymentWorkspaceUiCapabilities,
  getWorkspaceUiCapabilities,
} from "../lib/capabilities";
import type { AsyncState, WorkspaceSummary } from "../lib/types";

// ---------------------------------------------------------------------------
// ProjectsPage
// ---------------------------------------------------------------------------

const BASE_WORKSPACE_UI_CAPABILITIES = getWorkspaceUiCapabilities();

export const ProjectsPage: React.FC = () => {
  const navigate = useNavigate();
  const { user } = useAuth();
  const switchOrgThen = useSwitchOrgThen();
  const instanceInfo = useInstanceInfo();
  const capabilities = applyDeploymentWorkspaceUiCapabilities(
    BASE_WORKSPACE_UI_CAPABILITIES,
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  const showCloudWorkspaceWorkflows = capabilities.cloudWorkspaceWorkflows;
  const [search, setSearch] = useState("");
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [cloneSourceId, setCloneSourceId] = useState<string | null>(null);

  type WorkspaceWithOrg = WorkspaceSummary & {
    orgId: string;
    orgName: string;
    orgSlug: string;
  };

  const orgs = useMemo(() => user?.orgs ?? [], [user?.orgs]);
  // §38.167: Allow selecting which org to create workspaces in.
  // Default to the first org but track user selection.
  const [selectedOrgIndex, setSelectedOrgIndex] = useState(0);
  const activeOrg =
    orgs[Math.min(selectedOrgIndex, Math.max(0, orgs.length - 1))];
  const activeOrgId = activeOrg?.orgId ?? "";
  // Shares `workspacesQueryOptions` with `useWorkspaces` so this fan-out and
  // that hook register one queryFn per `["workspaces", orgId]` key rather than
  // two competing closures. The Rules of Hooks are why this can't just call
  // `useWorkspaces` in the map.
  const workspaceQueries = useQueries({
    queries: orgs.map((org) => workspacesQueryOptions(org.orgId)),
  });

  const workspacesState = useMemo<AsyncState<WorkspaceWithOrg[]>>(() => {
    if (orgs.length === 0) {
      return { status: "ok", data: [] };
    }

    const hasAnyData = workspaceQueries.some((query) =>
      Array.isArray(query.data),
    );
    const firstError =
      workspaceQueries.find((query) => query.error)?.error ??
      new Error("Failed to load projects.");

    if (!hasAnyData && workspaceQueries.some((query) => query.isPending)) {
      return { status: "loading" };
    }

    if (!hasAnyData && workspaceQueries.some((query) => query.isError)) {
      return { status: "error", error: firstError };
    }

    const data = orgs
      .flatMap((org, index) =>
        (workspaceQueries[index]?.data ?? []).map((workspace) => ({
          ...workspace,
          orgId: org.orgId,
          orgName: org.orgName,
          orgSlug: org.orgSlug,
        })),
      )
      .sort(
        (a, b) =>
          new Date(b.lastUpdated).getTime() - new Date(a.lastUpdated).getTime(),
      );

    return { status: "ok", data };
  }, [orgs, workspaceQueries]);

  const workspaces = useMemo(
    () => (workspacesState.status === "ok" ? workspacesState.data : []),
    [workspacesState],
  );
  const hasPartialWorkspaceError =
    workspacesState.status === "ok" &&
    workspaceQueries.some((query) => query.isError);

  const totalActive = useMemo(
    () => workspaces.reduce((s, w) => s + w.concurrentTasks, 0),
    [workspaces],
  );

  const handleSelectWorkspace = useCallback(
    (id: string) => {
      const ws = workspaces.find((w) => w.id === id);
      if (ws) {
        switchOrgThen.run(ws, () => {
          navigate(
            workspacePagePath(
              ws.orgSlug,
              ws.agentReview?.sourceWorkspaceName ?? ws.name,
            ) + (ws.agentReview ? "?view=approvals" : ""),
          );
        });
      }
    },
    [navigate, workspaces, switchOrgThen],
  );
  const handleOpenCreateWorkspace = useCallback(() => {
    if (!activeOrg) return;
    switchOrgThen.run(activeOrg, () => setCreateDialogOpen(true));
  }, [activeOrg, switchOrgThen]);
  const handleOpenCloneWorkspace = useCallback(
    (workspaceId: string) => {
      const workspace = workspaces.find(
        (candidate) => candidate.id === workspaceId,
      );
      if (!workspace || workspace.agentReview) return;
      switchOrgThen.run(workspace, () => setCloneSourceId(workspaceId));
    },
    [switchOrgThen, workspaces],
  );

  return (
    <AppShell>
      <TopBar>
        <span className="text-sm font-medium text-[var(--g5-text0)]">G5</span>
      </TopBar>

      <div className="flex-1 flex flex-col min-h-0 overflow-y-auto bg-[var(--g5-bg0)]">
        <div className="mx-auto w-full max-w-[1200px] px-4 py-6 space-y-5">
          {/* Header row */}
          <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
            <div className="flex items-center gap-3">
              <div className="w-3 h-3 rounded-full bg-[var(--g5-info)] shadow-[0_0_8px_rgba(103,164,255,0.5)]" />
              <h1 className="text-lg font-semibold text-[var(--g5-text0)]">
                Projects
              </h1>
            </div>
            <div className="flex items-center gap-4 text-sm text-[var(--g5-text2)]">
              <span className="flex items-center gap-1.5">
                <Layers className="w-4 h-4" />
                <span className="text-[var(--g5-text0)] font-medium">
                  {totalActive}
                </span>
              </span>
              {/* §38.167: Org selector for multi-org users */}
              {orgs.length > 1 && (
                <select
                  data-testid="projects-org-select"
                  value={selectedOrgIndex}
                  onChange={(e) => setSelectedOrgIndex(Number(e.target.value))}
                  className="px-2 py-1.5 rounded-md text-xs bg-[var(--g5-bg2)] text-[var(--g5-text0)] border border-[var(--g5-stroke)]"
                  aria-label="Select organization for new project"
                >
                  {orgs.map((org, idx) => (
                    <option key={org.orgId} value={idx}>
                      {org.orgName}
                    </option>
                  ))}
                </select>
              )}
              {showCloudWorkspaceWorkflows && orgs.length > 0 && (
                <button
                  type="button"
                  data-testid="projects-new-project"
                  onClick={handleOpenCreateWorkspace}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[var(--g5-info)] text-[var(--g5-bg0)] text-xs font-medium hover:opacity-90 transition-opacity cursor-pointer"
                >
                  <GitBranch className="w-3.5 h-3.5" />
                  New Project
                </button>
              )}
            </div>
          </div>

          {/* Loading / Error states */}
          {workspacesState.status === "loading" && (
            <p className="text-sm text-[var(--g5-text2)]">Loading projects…</p>
          )}
          {workspacesState.status === "error" && (
            <p className="text-sm text-[var(--g5-blocked)]">
              Failed to load projects.
            </p>
          )}
          {hasPartialWorkspaceError && (
            <p className="text-sm text-[var(--g5-text2)]">
              Some organizations could not be loaded. Showing the projects that
              were available.
            </p>
          )}
          {switchOrgThen.error && (
            <p role="alert" className="text-sm text-[var(--g5-blocked)]">
              {switchOrgThen.error}
            </p>
          )}

          {/* Search */}
          <div className="relative max-w-md">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--g5-text2)]" />
            <input
              type="text"
              placeholder="Search workspaces…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full pl-10 pr-4 py-2 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-sm text-[var(--g5-text1)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]/50"
            />
          </div>

          {/* Table */}
          <WorkspaceTable
            workspaces={workspaces}
            searchQuery={search}
            onSelectWorkspace={handleSelectWorkspace}
            onPauseWorkspace={() => {}}
            onAbortWorkspace={() => {}}
            onCloneWorkspace={
              showCloudWorkspaceWorkflows ? handleOpenCloneWorkspace : () => {}
            }
            onNewWorkspace={() => {
              if (showCloudWorkspaceWorkflows) handleOpenCreateWorkspace();
            }}
            capabilities={capabilities}
          />
        </div>
      </div>

      {/* Create Workspace Dialog — conditionally rendered so each open
          gets a fresh component instance with default state.  This avoids
          the useEffect-based form-reset race condition where the async
          setRepos([]) could fire AFTER a user click updates repos. */}
      {showCloudWorkspaceWorkflows && createDialogOpen && (
        <CreateWorkspaceDialog
          open
          onClose={() => setCreateDialogOpen(false)}
          orgId={activeOrgId}
          onCreated={(workspaceName, landing) => {
            if (!activeOrg) return;
            switchOrgThen.run(activeOrg, () => {
              navigate(
                `${workspacePagePath(activeOrg.orgSlug, workspaceName)}${forkLandingQuery(landing)}`,
                { state: forkLandingState(landing) },
              );
            });
          }}
        />
      )}

      {/* Clone Workspace Dialog */}
      {showCloudWorkspaceWorkflows &&
        cloneSourceId &&
        (() => {
          const cloneSource = workspaces.find((w) => w.id === cloneSourceId);
          return cloneSource ? (
            <CloneDialog
              source={cloneSource}
              orgId={cloneSource.orgId}
              onCloned={(workspace, landing) => {
                switchOrgThen.run(cloneSource, () => {
                  navigate(
                    `${workspacePagePath(cloneSource.orgSlug, workspace.name)}${forkLandingQuery(landing)}`,
                    { state: forkLandingState(landing) },
                  );
                });
              }}
              onClose={() => setCloneSourceId(null)}
            />
          ) : null;
        })()}
    </AppShell>
  );
};

export default ProjectsPage;
