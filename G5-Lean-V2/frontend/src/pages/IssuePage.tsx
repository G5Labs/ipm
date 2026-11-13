// =============================================================================
// Issue page — one issue outside any workspace
// =============================================================================
// `/:org/issues/github/:repositoryId/:number` and, for the native G5
// tracker, `/:org/issues/g5/:number` (g5.workbench.issues.create): where
// Home's Create lands for the new issue. The issue view is the workspace
// tab's `ExternalIssueDetailPanel`, which on the page also carries the
// issue's write controls for G5 and GitHub issues alike — edit, close or
// reopen, comment, edit one's own comments (g5.issue.native.page,
// g5.external-issues.edit); the page adds a way back to Home, a way back to
// the workspace that holds the issue as primary when the viewer can see it,
// and Start workspace (the fork dialog with the issue preselected) while no
// workspace owns the issue. Like Home, it switches the
// session to the route's organization before organization-scoped reads.
// =============================================================================

import {
  NATIVE_ISSUE_REPOSITORY_ID,
  parseIssueNumberParam,
} from "../lib/issuePage";
import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Plus } from "lucide-react";
import { Link, Navigate, useNavigate, useParams } from "react-router-dom";

import { AppShell } from "../components/layout/Layout";
import { ExternalIssueDetailPanel } from "../components/external-issues/ExternalIssueDetailPanel";
import { CreateWorkspaceDialog } from "../components/workspace/CreateWorkspaceDialog";
import { Button } from "../components/ui/UI";
import { useAuth } from "../hooks/useAuth";
import {
  useExternalIssueDetail,
  useExternalIssuesFeature,
} from "../hooks/useExternalIssues";
import { useInstanceInfo } from "../hooks/useSettings";
import {
  applyDeploymentWorkspaceUiCapabilities,
  getWorkspaceUiCapabilities,
} from "../lib/capabilities";
import { forkLandingQuery, forkLandingState } from "../lib/workspaceLanding";

const BASE_WORKSPACE_UI_CAPABILITIES = getWorkspaceUiCapabilities();

export function IssuePage() {
  // The native route has no repository segment.
  const {
    org: routeOrgSlug,
    repositoryId = NATIVE_ISSUE_REPOSITORY_ID,
    number: numberParam = "",
  } = useParams<{ org: string; repositoryId: string; number: string }>();
  const { user, switchOrg } = useAuth();
  const instanceInfo = useInstanceInfo();
  const navigate = useNavigate();
  const [startingWorkspace, setStartingWorkspace] = useState(false);
  // A failed switch, for the organization it was attempted for.
  const [failedSwitch, setFailedSwitch] = useState<string | null>(null);
  const attemptedOrgRef = useRef<string | null>(null);

  const number = parseIssueNumberParam(numberParam);
  const routeOrg = useMemo(
    () => user?.orgs.find((membership) => membership.orgSlug === routeOrgSlug),
    [routeOrgSlug, user],
  );
  const routeOrgActive =
    !!routeOrg?.orgId && user?.activeOrgId === routeOrg.orgId;

  useEffect(() => {
    if (!routeOrg || routeOrgActive) {
      if (routeOrgActive) attemptedOrgRef.current = null;
      return;
    }
    if (attemptedOrgRef.current === routeOrg.orgId) return;
    attemptedOrgRef.current = routeOrg.orgId;
    void switchOrg(routeOrg.orgId).catch(() => setFailedSwitch(routeOrg.orgId));
  }, [routeOrg, routeOrgActive, switchOrg]);
  const orgSwitchError =
    routeOrg && failedSwitch === routeOrg.orgId
      ? `Could not switch to ${routeOrg.orgName}.`
      : null;

  const feature = useExternalIssuesFeature(routeOrg?.orgId);
  // The panel reads the same query; the page only needs its owner.
  const detail = useExternalIssueDetail(
    routeOrg?.orgId ?? "",
    repositoryId,
    number,
    feature.status === "enabled",
  );
  const capabilities = applyDeploymentWorkspaceUiCapabilities(
    BASE_WORKSPACE_UI_CAPABILITIES,
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );

  if (!user) return <Navigate to="/login" replace />;

  if (!routeOrg || !routeOrgActive) {
    return (
      <AppShell>
        <main className="flex-1 bg-[var(--g5-bg0)] px-4 py-8">
          <p
            role={!routeOrg || orgSwitchError ? "alert" : "status"}
            className="mx-auto max-w-[1200px] text-sm text-[var(--g5-text2)]"
          >
            {!routeOrg
              ? "You do not have access to this organization."
              : orgSwitchError
                ? `${orgSwitchError} Try again from the organization switcher.`
                : `Switching to ${routeOrg.orgName}…`}
          </p>
        </main>
      </AppShell>
    );
  }

  const issue = detail.status === "ok" ? detail.data : null;
  const canStartWorkspace =
    capabilities.cloudWorkspaceWorkflows &&
    issue !== null &&
    issue.primaryWorkspace === null;
  const displayKey = issue?.displayKey ?? `#${numberParam}`;
  const primaryWorkspace =
    issue?.primaryWorkspace?.visible === true ? issue.primaryWorkspace : null;

  return (
    <AppShell>
      <div className="flex-1 overflow-y-auto bg-[var(--g5-bg0)]">
        <div className="flex min-w-0 w-full flex-wrap items-center justify-between gap-3 px-6 pt-6">
          <Link
            to={`/${routeOrg.orgSlug}/home`}
            data-testid="issue-page-back"
            className="inline-flex items-center gap-1 text-sm text-[var(--g5-text2)] no-underline hover:text-[var(--g5-text0)]"
          >
            <ArrowLeft size={14} aria-hidden="true" />
            {routeOrg.orgName} Home
          </Link>
          {primaryWorkspace && (
            <Link
              to={`/${routeOrg.orgSlug}/${encodeURIComponent(primaryWorkspace.name)}`}
              data-testid="issue-page-back-to-workspace"
              title={`Back to workspace ${primaryWorkspace.name}, where this is the primary issue`}
              className="mr-auto inline-flex min-w-0 items-center gap-1 text-sm text-[var(--g5-text2)] no-underline hover:text-[var(--g5-text0)]"
            >
              <ArrowLeft size={14} aria-hidden="true" />
              <span className="truncate">Back to {primaryWorkspace.name}</span>
            </Link>
          )}
          {canStartWorkspace && (
            <Button
              kind="primary"
              data-testid="issue-page-start-workspace"
              aria-label={`Start a workspace for ${displayKey}`}
              onClick={() => setStartingWorkspace(true)}
            >
              <Plus size={14} aria-hidden="true" />
              Start workspace
            </Button>
          )}
        </div>
        <ExternalIssueDetailPanel
          repositoryId={repositoryId}
          number={number}
          name={displayKey}
          issuePage
        />
      </div>
      {capabilities.cloudWorkspaceWorkflows && startingWorkspace && issue && (
        <CreateWorkspaceDialog
          open
          orgId={routeOrg.orgId}
          initialIssueRef={issue.displayKey}
          onClose={() => setStartingWorkspace(false)}
          onCreated={(workspaceName, landing) => {
            setStartingWorkspace(false);
            // As from Home: on Chat, with the chat the fork started or the
            // issue's suggested first message pre-typed.
            navigate(
              `/${routeOrg.orgSlug}/${encodeURIComponent(workspaceName)}${forkLandingQuery(landing)}`,
              { state: forkLandingState(landing) },
            );
          }}
        />
      )}
    </AppShell>
  );
}
