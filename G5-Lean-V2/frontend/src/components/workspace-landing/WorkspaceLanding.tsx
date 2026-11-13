// =============================================================================
// Workspace landing
// =============================================================================
// The landing above Activity (ui g5.workbench.landing): the personal request
// banner when it applies, the summary line, the Primary Issue and the Work
// Description, in that order. The engineering phase stays in the phase
// chevrons; nothing here repeats it.
// =============================================================================

import { useState } from "react";
import { useAuth } from "../../hooks/useAuth";
import type { ApprovalSummary } from "../../lib/landingStatus";
import { PersonalApprovalsBanner } from "../workspace-review/PersonalApprovalsBanner";
import {
  ImplementationSection,
  useLandingPullRequest,
} from "./ImplementationSection";
import { LandingSummary } from "./LandingSummary";
import { WorkspaceTaskSection } from "./WorkspaceTaskSection";

export function WorkspaceLanding({
  workspaceId,
  workspaceName,
  description,
  docsRepoId,
  approvals,
}: {
  workspaceId: string;
  workspaceName: string;
  description: string | null;
  docsRepoId: string | null;
  /** `null` where the workspace offers no Approvals destination. */
  approvals: ApprovalSummary | null;
}) {
  const { user } = useAuth();
  // Primary Issue starts collapsed only while Work Description presents a PR;
  // loading, unavailable and empty states keep it open. A person's choice
  // holds for this workspace's landing and is never saved.
  const { pull } = useLandingPullRequest(workspaceId);
  const [manual, setManual] = useState<{
    workspaceId: string;
    open: boolean;
  } | null>(null);
  const issueOpen = manual?.workspaceId === workspaceId ? manual.open : !pull;
  return (
    <div data-testid="workspace-landing">
      {approvals && (
        <PersonalApprovalsBanner
          workspaceId={workspaceId}
          viewerUserId={user?.id ?? ""}
          navigation
        />
      )}
      <LandingSummary workspaceId={workspaceId} approvals={approvals} />
      <WorkspaceTaskSection
        workspaceId={workspaceId}
        workspaceName={workspaceName}
        description={description}
        docsRepoId={docsRepoId}
        disclosure={{
          open: issueOpen,
          onToggle: () => setManual({ workspaceId, open: !issueOpen }),
        }}
      />
      <ImplementationSection workspaceId={workspaceId} />
    </div>
  );
}
