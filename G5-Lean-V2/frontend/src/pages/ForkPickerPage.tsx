// =============================================================================
// G5 Client — Organization fork picker ("+" in the tab strip)
// =============================================================================
// The tab strip's New tab (workbench g5.workbench.tabs; mock story B4): the
// route's organization (`/:org/fork`), with the person's role, and its
// workspaces as a lineage tree under Base. A workspace's name opens it as the
// new tab; Fork switches the session to the organization first and runs the
// ordinary fork dialog, and the new workspace opens as the new tab. Reachable
// only through "+", so the page carries no layout branch of its own.
// =============================================================================

import { useQuery } from "@tanstack/react-query";
import { GitFork, Plus } from "lucide-react";
import React, { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { AppShell } from "../components/layout/Layout";
import { ShellControls } from "../components/layout/ShellControls";
import { SearchField } from "../components/ui/SearchField";
import { WorkspaceLineageRow } from "../components/workspace/WorkspaceLineageRow";
import { UserSettingsPanel } from "../components/settings/UserSettingsPanel";
import { Button, Chip } from "../components/ui/UI";
import { CloneDialog } from "../components/workspace/CloneDialog";
import { CreateWorkspaceDialog } from "../components/workspace/CreateWorkspaceDialog";
import { useRouteOrgMembership } from "../hooks/useExternalIssues";
import { useSwitchOrgThen } from "../hooks/useSwitchOrgThen";
import { useInstanceInfo } from "../hooks/useSettings";
import { workspacesQueryOptions } from "../hooks/useWorkspace";
import {
  applyDeploymentWorkspaceUiCapabilities,
  getWorkspaceUiCapabilities,
} from "../lib/capabilities";
import { forkLandingQuery, forkLandingState } from "../lib/workspaceLanding";
import { workspacePagePath } from "../lib/workspacePagePath";
import {
  buildWorkspaceTreeRows,
  filterWorkspaceTreeRows,
} from "../lib/workspaceTree";
import type { CurrentUserDTO, WorkspaceSummary } from "../lib/types";

type OrgMembership = CurrentUserDTO["orgs"][number];

interface ForkSource {
  org: OrgMembership;
  workspace: WorkspaceSummary;
}

const BASE_WORKSPACE_UI_CAPABILITIES = getWorkspaceUiCapabilities();

function roleLabel(role: string): string {
  return role ? role.charAt(0).toUpperCase() + role.slice(1) : "Member";
}

export const ForkPickerPage: React.FC = () => {
  const navigate = useNavigate();
  const { org: routeOrgSlug } = useParams<{ org: string }>();
  const switchOrgThen = useSwitchOrgThen();
  const instanceInfo = useInstanceInfo();
  const instanceData = instanceInfo.status === "ok" ? instanceInfo.data : null;
  const capabilities = applyDeploymentWorkspaceUiCapabilities(
    BASE_WORKSPACE_UI_CAPABILITIES,
    instanceData,
  );
  const canFork = capabilities.cloudWorkspaceWorkflows;

  // The route's organization, among the person's memberships; a slug outside
  // them lists nothing.
  const org = useRouteOrgMembership();
  // Shares `useWorkspaces`'s query identity.
  const query = useQuery(workspacesQueryOptions(org?.orgId ?? ""));

  const [forkSource, setForkSource] = useState<ForkSource | null>(null);
  // New workspace (from the organization Base), as on Home; the organization
  // is switched first like a fork (g5.workbench.tabs).
  const [createOrg, setCreateOrg] = useState<OrgMembership | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // The search narrows the tree, matches with their ancestors
  // (g5.workbench.shell-chrome).
  const [search, setSearch] = useState("");

  // A fork runs under the source's organization: switch the session first,
  // as the projects page does, then open the ordinary fork dialog.
  const requestFork = (org: OrgMembership, workspace: WorkspaceSummary) =>
    switchOrgThen.run(org, () => setForkSource({ org, workspace }));
  const requestCreate = (org: OrgMembership) =>
    switchOrgThen.run(org, () => setCreateOrg(org));

  // Other people's review copies are never forked, so they are not offered
  // (g5.governance.review-workspace.exclusions).
  const allRows = Array.isArray(query.data)
    ? buildWorkspaceTreeRows(
        query.data.filter((workspace) => workspace.isReview !== true),
        { baseFirst: true },
      )
    : [];
  const rows = filterWorkspaceTreeRows(allRows, search);

  return (
    <AppShell
      // Like Home, the picker offers the Account flyout and the person's
      // Settings in the strip; Settings lands on the route org's Base
      // settings tab through `/:org/settings` (ui g5.ui.settings-page).
      shellControls={
        <ShellControls
          onOpenAccount={() => setSettingsOpen(true)}
          onOpenSettings={
            routeOrgSlug
              ? () => navigate(`/${routeOrgSlug}/settings`)
              : undefined
          }
        />
      }
    >
      <main
        data-testid="fork-picker"
        className="flex-1 min-h-0 overflow-y-auto bg-[var(--g5-bg0)]"
      >
        <div className="mx-auto w-full max-w-[880px] px-4 py-6">
          {/* Organizations are created from the Account flyout, not here
              (g5.workbench.tabs: the picker opens, forks and creates
              workspaces). */}
          <div className="flex items-center justify-between gap-3">
            <h1 className="text-xs font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
              Workspaces
            </h1>
            {canFork && org && (
              <Button
                data-testid="fork-picker-new-workspace"
                kind="primary"
                disabled={switchOrgThen.pendingOrgId !== null}
                onClick={() => requestCreate(org)}
              >
                <Plus className="mr-1 h-3.5 w-3.5" />
                New workspace
              </Button>
            )}
          </div>
          <p className="mt-1 text-sm text-[var(--g5-text2)]">
            {canFork
              ? "Choose a workspace to open or fork into a new tab."
              : "Forking is not available on this deployment."}
          </p>
          <SearchField
            testId="fork-picker-search"
            label="Search workspaces"
            placeholder="Search workspaces..."
            value={search}
            onChange={setSearch}
            className="mt-3"
          />
          {switchOrgThen.error && (
            <p
              data-testid="fork-picker-switch-error"
              role="alert"
              className="mt-3 text-sm text-[var(--g5-blocked)]"
            >
              {switchOrgThen.error}
            </p>
          )}

          <div className="mt-4 flex flex-col gap-3">
            {!org && (
              <p
                data-testid="fork-picker-not-member"
                className="text-sm text-[var(--g5-text2)]"
              >
                You are not a member of this organization.
              </p>
            )}
            {org && (
              <section
                data-testid="fork-picker-org"
                data-org-slug={org.orgSlug}
                className="rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4"
              >
                <header className="flex items-center gap-2">
                  <span className="text-sm font-semibold text-[var(--g5-text0)]">
                    {org.orgName}
                  </span>
                  <Chip state="muted" text={roleLabel(org.role)} />
                </header>
                <div className="mt-2">
                  {query.isPending && !query.data && (
                    <p className="px-1 py-1.5 text-xs text-[var(--g5-text2)]">
                      Loading workspaces…
                    </p>
                  )}
                  {query.isError && !query.data && (
                    <p
                      data-testid="fork-picker-org-error"
                      className="flex items-center gap-2 px-1 py-1.5 text-xs text-[var(--g5-blocked)]"
                    >
                      Could not load this organization&rsquo;s workspaces.
                      <button
                        type="button"
                        data-testid="fork-picker-org-retry"
                        className="cursor-pointer text-[var(--g5-info)] hover:underline"
                        onClick={() => void query.refetch()}
                      >
                        Retry
                      </button>
                    </p>
                  )}
                  {Array.isArray(query.data) && rows.length === 0 && (
                    <p
                      data-testid="fork-picker-org-empty"
                      className="px-1 py-1.5 text-xs text-[var(--g5-text2)]"
                    >
                      {search.trim()
                        ? `No workspace matches "${search.trim()}".`
                        : "No workspaces."}
                    </p>
                  )}
                  {rows.map(({ workspace, depth }) => (
                    <WorkspaceLineageRow
                      key={workspace.id}
                      workspace={workspace}
                      depth={depth}
                      testId="fork-picker-row"
                      // The name opens the workspace as the new tab, as on
                      // Home's directory; modified clicks are the browser's.
                      to={workspacePagePath(org.orgSlug, workspace.name)}
                      linkTestId="fork-picker-open"
                    >
                      {canFork && (
                        <Button
                          data-testid="fork-picker-fork"
                          kind="secondary"
                          disabled={switchOrgThen.pendingOrgId !== null}
                          aria-label={`Fork ${workspace.name}`}
                          onClick={() => requestFork(org, workspace)}
                        >
                          <GitFork className="mr-1 h-3.5 w-3.5" />
                          Fork
                        </Button>
                      )}
                    </WorkspaceLineageRow>
                  ))}
                </div>
              </section>
            )}
          </div>
        </div>
      </main>

      {forkSource && (
        <CloneDialog
          source={forkSource.workspace}
          orgId={forkSource.org.orgId}
          onCloned={(workspace, landing) => {
            const orgSlug = forkSource.org.orgSlug;
            setForkSource(null);
            // Landing on the workspace route opens its tab and retires the
            // transient New tab (g5.workbench.tabs).
            navigate(
              `${workspacePagePath(orgSlug, workspace.name)}${forkLandingQuery(landing)}`,
              { state: forkLandingState(landing) },
            );
          }}
          onClose={() => setForkSource(null)}
        />
      )}
      {createOrg && (
        <CreateWorkspaceDialog
          open
          orgId={createOrg.orgId}
          onClose={() => setCreateOrg(null)}
          onCreated={(workspaceName, landing) => {
            const orgSlug = createOrg.orgSlug;
            setCreateOrg(null);
            navigate(
              `${workspacePagePath(orgSlug, workspaceName)}${forkLandingQuery(landing)}`,
              { state: forkLandingState(landing) },
            );
          }}
        />
      )}
      {settingsOpen && (
        <UserSettingsPanel
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          workspaceId=""
        />
      )}
    </AppShell>
  );
};
