// The caller's personal workspace state shared by Home's personal surfaces
// (personal_workspace g5.personal.home); `PersonalHomeProvider` supplies it.

import { createContext, useContext } from "react";

import type {
  usePersonalWorkspace,
  useWorkspace,
  useWorkspaceVm,
} from "../../hooks/useWorkspace";
import type { WorkspaceLiveGateValue } from "../../lib/workspaceLiveGate";
import type { WorkspaceDetail } from "../../lib/types";

export interface PersonalHomeState {
  orgId: string;
  orgSlug: string;
  personal: ReturnType<typeof usePersonalWorkspace>;
  workspaceId: string;
  workspace: ReturnType<typeof useWorkspace>;
  detail: WorkspaceDetail | null;
  vm: ReturnType<typeof useWorkspaceVm>;
  live: WorkspaceLiveGateValue;
  mePath: string;
  start: () => Promise<void>;
  starting: boolean;
  startError: string | null;
}

export const PersonalHomeContext = createContext<PersonalHomeState | null>(
  null,
);

/** The shared personal state, or `null` outside a provider. */
export function usePersonalHome(): PersonalHomeState | null {
  return useContext(PersonalHomeContext);
}
