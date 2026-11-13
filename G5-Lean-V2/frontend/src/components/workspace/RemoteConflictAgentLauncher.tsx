import { useEffect, useMemo, useRef, useState } from "react";
import { useResolveAgentLaunch } from "../../hooks/useAgentLaunch";
import { useWorkspaceScopedConfig } from "../../hooks/useSettings";
import { useUserSettings } from "../../hooks/useUserSettings";
import { useWorkspace } from "../../hooks/useWorkspace";
import {
  defaultAgentProviderFromConfig,
  getAvailableSyncAgentConfigs,
  getPreferredSyncAgentConfig,
} from "../../lib/agentLaunchDefaults";
import type { AgentLaunchSelectionV1 } from "../../lib/types";
import { GatedControl, useLiveGateReason } from "../../lib/workspaceLiveGate";
import {
  AgentLaunchConfigControls,
  initialAgentConfig,
  reconcileAgentConfig,
} from "../agent/AgentLaunchConfigControls";

/** Use the same visible, confirmed launch choices as Base conflict resolution. */
export function RemoteConflictAgentLauncher({
  workspaceId,
  launchId,
  starting,
  onStart,
}: {
  workspaceId: string;
  launchId: string;
  starting: boolean;
  onStart: (selection: AgentLaunchSelectionV1) => Promise<void>;
}) {
  const workspace = useWorkspace(workspaceId);
  // Resolution runs in the already-running owner VM
  // (`g5.governance.remote.divergence`): while it is stopped, the owner
  // reads why and gets the header's Start.
  const resolveBlocked = useLiveGateReason("resolve with an agent");
  const scopedConfig = useWorkspaceScopedConfig(
    workspaceId,
    workspace.status === "ok" ? workspace.data.orgId : "",
  );
  const { settings } = useUserSettings();
  const choices = settings.defaultAgentProviderChoices;
  const options = useMemo(
    () => getAvailableSyncAgentConfigs(choices),
    [choices],
  );
  const preferred = useMemo(
    () =>
      getPreferredSyncAgentConfig(
        defaultAgentProviderFromConfig(
          scopedConfig.status === "ok"
            ? scopedConfig.data.effectiveSettings
            : null,
        ),
        choices,
      ),
    [scopedConfig, choices],
  );
  const [config, setConfig] = useState(() =>
    initialAgentConfig(preferred, options),
  );
  const userPicked = useRef(false);
  useEffect(() => {
    setConfig((previous) =>
      reconcileAgentConfig(previous, userPicked.current, preferred, options),
    );
  }, [preferred, options]);
  const resolve = useResolveAgentLaunch(workspaceId);
  const [selection, setSelection] = useState<AgentLaunchSelectionV1 | null>(
    null,
  );
  const ready = workspace.status === "ok" && scopedConfig.status === "ok";
  const error =
    workspace.status === "error"
      ? workspace.error
      : scopedConfig.status === "error"
        ? scopedConfig.error
        : null;

  return (
    <div className="space-y-2">
      {ready ? (
        <AgentLaunchConfigControls
          value={config}
          options={options}
          onChange={(next) => {
            userPicked.current = true;
            setSelection(null);
            setConfig(next);
          }}
          resolvePreferences={resolve.mutateAsync}
          onResolutionSelectionChange={setSelection}
          disabled={starting}
          testIdPrefix={`resolve-merge-${launchId}`}
          selectVariant="native"
          columns={2}
        />
      ) : error ? (
        <p role="alert" className="text-[11px] text-[var(--g5-blocked)]">
          Could not load agent settings: {error.message}
        </p>
      ) : (
        <p className="text-[11px] text-[var(--g5-text2)]">
          Loading agent settings…
        </p>
      )}
      <GatedControl
        verb="resolve with an agent"
        reason={resolveBlocked}
        testId={`start-merge-${launchId}-hint`}
        className="w-full"
      >
        <button
          type="button"
          data-testid={`start-merge-${launchId}`}
          onClick={() => {
            if (ready && selection && !starting) void onStart(selection);
          }}
          disabled={starting || !ready || !selection || resolveBlocked !== null}
          title={resolveBlocked ?? undefined}
          className="w-full rounded bg-[var(--g5-text0)] px-2 py-1 text-xs font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
        >
          {starting ? "Starting…" : "Resolve with agent"}
        </button>
      </GatedControl>
    </div>
  );
}
