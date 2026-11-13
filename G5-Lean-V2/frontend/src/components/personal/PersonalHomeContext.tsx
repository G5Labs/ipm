// =============================================================================
// G5 Client — the caller's personal workspace on Home, read once
// =============================================================================
// Home shows the personal workspace twice: its status and My files in the main
// column, and its Chat in a side panel (personal_workspace g5.personal.home).
// Both read one state: the workspace, its VM, the workbench's live gate
// (`useWorkspaceLiveGateValue`) and Start. Start asked for through the shared
// `WORKSPACE_START_EVENT` is handled here, once, because no workbench is
// mounted on Home.
// =============================================================================

import React, { useCallback, useEffect, useState } from "react";

import {
  usePersonalWorkspace,
  useResumeWorkspaceVm,
  useWorkspace,
  useWorkspaceVm,
} from "../../hooks/useWorkspace";
import {
  liveGateCanStart,
  useWorkspaceLiveGateValue,
} from "../../lib/workspaceLiveGate";
import { personalWorkspaceMePath } from "../../lib/workspacePagePath";
import { WORKSPACE_START_EVENT } from "../workspace/reviewWorkspace";
import {
  PersonalHomeContext,
  type PersonalHomeState,
} from "./personalHomeState";

export interface PersonalHomeProviderProps {
  orgId: string;
  orgSlug: string;
  userId: string;
  /** The caller administers the organization (gate wording only). */
  orgAdmin: boolean;
  children: React.ReactNode;
}

export function PersonalHomeProvider({
  orgId,
  orgSlug,
  userId,
  orgAdmin,
  children,
}: PersonalHomeProviderProps) {
  const personal = usePersonalWorkspace(orgId);
  const workspaceId = personal.status === "ok" ? personal.data.workspaceId : "";
  const workspace = useWorkspace(workspaceId);
  const vm = useWorkspaceVm(workspaceId);
  const detail =
    workspace.status === "ok" && workspace.data.id === workspaceId
      ? workspace.data
      : null;
  // The workbench's gate, from the same derivation (g5.personal.home).
  const live = useWorkspaceLiveGateValue({
    workspace: detail,
    vmStatus: vm.status === "ok" ? vm.data.status : null,
    userId,
    orgAdmin,
    reviewCopy: false,
    reviewIdentityKnown: true,
  });
  const resume = useResumeWorkspaceVm(workspaceId);
  const resumeWorkspace = resume.mutateAsync;
  const [startError, setStartError] = useState<string | null>(null);
  const start = useCallback(async () => {
    setStartError(null);
    try {
      await resumeWorkspace(undefined);
    } catch (error) {
      setStartError(
        error instanceof Error && error.message
          ? error.message
          : "Could not start your personal workspace.",
      );
    }
  }, [resumeWorkspace]);
  const ownerCanStart = live.isOwner && liveGateCanStart(live.kind);
  useEffect(() => {
    if (!live.isOwner || !workspaceId) return;
    const onStart = () => {
      if (ownerCanStart) void start();
    };
    window.addEventListener(WORKSPACE_START_EVENT, onStart);
    return () => window.removeEventListener(WORKSPACE_START_EVENT, onStart);
  }, [live.isOwner, ownerCanStart, start, workspaceId]);

  const value: PersonalHomeState = {
    orgId,
    orgSlug,
    personal,
    workspaceId,
    workspace,
    detail,
    vm,
    live,
    mePath: personalWorkspaceMePath(orgSlug),
    start,
    starting: resume.isLoading,
    startError,
  };
  return (
    <PersonalHomeContext.Provider value={value}>
      {children}
    </PersonalHomeContext.Provider>
  );
}
