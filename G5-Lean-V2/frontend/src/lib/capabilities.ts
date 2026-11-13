// =============================================================================
// G5 Client — UI Capability Gates
// =============================================================================
// Controls visibility of workflow surfaces that depend on project/task
// orchestration. Production defaults these off unless explicitly enabled; tests
// may pass explicit minimal capabilities to exercise reduced-surface UI.
// =============================================================================

export interface WorkspaceUiCapabilities {
  cloudWorkspaceWorkflows: boolean;
  intentLogViewer: boolean;
}

export const MINIMAL_WORKSPACE_UI_CAPABILITIES: WorkspaceUiCapabilities = {
  cloudWorkspaceWorkflows: false,
  intentLogViewer: false,
};

export const CLOUD_WORKSPACE_UI_CAPABILITIES: WorkspaceUiCapabilities = {
  cloudWorkspaceWorkflows: true,
  intentLogViewer: false,
};

type CapabilityEnv = Pick<
  ImportMetaEnv,
  | "MODE"
  | "VITE_G5_CI_WORKFLOW"
  | "VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS"
  | "VITE_G5_ENABLE_INTENT_LOG_VIEWER"
>;

function envFlagDisabled(value: string | undefined): boolean {
  return value === "0" || value === "false" || value === "no";
}

function envFlagEnabled(value: string | undefined): boolean {
  return value === "1" || value === "true" || value === "yes";
}

export function resolveWorkspaceUiCapabilities(
  env: Partial<CapabilityEnv>,
): WorkspaceUiCapabilities {
  const cloudWorkspaceWorkflows =
    !envFlagDisabled(env.VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS) &&
    (env.MODE === "test" ||
      envFlagEnabled(env.VITE_G5_CI_WORKFLOW) ||
      envFlagEnabled(env.VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS));

  return {
    cloudWorkspaceWorkflows,
    intentLogViewer: envFlagEnabled(env.VITE_G5_ENABLE_INTENT_LOG_VIEWER),
  };
}

export function getWorkspaceUiCapabilities(): WorkspaceUiCapabilities {
  return resolveWorkspaceUiCapabilities(import.meta.env);
}

export function applyDeploymentWorkspaceUiCapabilities(
  capabilities: WorkspaceUiCapabilities,
  deployment: { selfHosted?: boolean } | null | undefined,
): WorkspaceUiCapabilities {
  return {
    ...capabilities,
    cloudWorkspaceWorkflows:
      capabilities.cloudWorkspaceWorkflows && deployment?.selfHosted === false,
  };
}
