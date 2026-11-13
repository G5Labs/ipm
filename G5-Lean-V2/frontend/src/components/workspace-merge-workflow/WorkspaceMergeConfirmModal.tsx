import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, GitMerge, X } from "lucide-react";
import type { AgentLaunchSelectionV1, SyncAgentConfig } from "../../lib/types";
import {
  AgentLaunchConfigControls,
  initialAgentConfig,
  reconcileAgentConfig,
} from "../agent/AgentLaunchConfigControls";
import type { ResolveAgentLaunchPreferences } from "../agent/AgentLaunchConfigControls";
import { classNames, Modal, ThemedSelect } from "../ui/UI";
import { usePublicationBlockedReason } from "../../lib/workspaceLiveGate";
import { RebaseChangeGraphPreview } from "./RebaseChangeGraphPreview";

const PHASES = [
  "Lineage Preflight",
  "Compute Source Rebase",
  "Update Source Branches",
  "Refresh Workspace State",
];

export interface WorkspaceMergeTargetOption {
  id: string;
  name: string;
}

export interface WorkspaceMergeConfirmModalProps {
  open: boolean;
  sourceWorkspaceId: string;
  sourceWorkspaceName: string;
  targets: WorkspaceMergeTargetOption[];
  targetWorkspaceId: string;
  agentConfig: SyncAgentConfig;
  agentConfigOptions?: SyncAgentConfig[];
  resolveAgentLaunch: ResolveAgentLaunchPreferences;
  isSubmitting?: boolean;
  errorMessage?: string | null;
  onTargetWorkspaceChange: (workspaceId: string) => void;
  onCancel: () => void;
  onConfirm: (
    agentConfig: SyncAgentConfig,
    agentLaunch: AgentLaunchSelectionV1,
  ) => void;
}

export function WorkspaceMergeConfirmModal({
  open,
  sourceWorkspaceId,
  sourceWorkspaceName,
  targets,
  targetWorkspaceId,
  agentConfig,
  agentConfigOptions,
  resolveAgentLaunch,
  isSubmitting = false,
  errorMessage = null,
  onTargetWorkspaceChange,
  onCancel,
  onConfirm,
}: WorkspaceMergeConfirmModalProps) {
  const [selectedAgentConfig, setSelectedAgentConfig] =
    useState<SyncAgentConfig>(() =>
      initialAgentConfig(agentConfig, agentConfigOptions),
    );
  const [confirmedAgentLaunch, setConfirmedAgentLaunch] =
    useState<AgentLaunchSelectionV1 | null>(null);
  // Untouched selections follow the preferred config when it changes mid-open.
  const userPickedAgentConfigRef = useRef(false);
  const handleAgentConfigChange = useCallback((config: SyncAgentConfig) => {
    userPickedAgentConfigRef.current = true;
    setSelectedAgentConfig(config);
  }, []);
  const wasOpenRef = useRef(false);
  const targetOptions = useMemo(
    () => [
      { value: "", label: "Select target" },
      ...targets.map((target) => ({
        value: target.id,
        label: target.name,
      })),
    ],
    [targets],
  );
  const targetWorkspaceName =
    targets.find((target) => target.id === targetWorkspaceId)?.name ??
    "target workspace";

  useEffect(() => {
    const wasOpen = wasOpenRef.current;
    if (open && !wasOpen) {
      userPickedAgentConfigRef.current = false;
      setSelectedAgentConfig(
        initialAgentConfig(agentConfig, agentConfigOptions),
      );
    }
    wasOpenRef.current = open;
  }, [open, agentConfig, agentConfigOptions]);

  useEffect(() => {
    if (!open) return;
    setSelectedAgentConfig((previous) =>
      reconcileAgentConfig(
        previous,
        userPickedAgentConfigRef.current,
        agentConfig,
        agentConfigOptions,
      ),
    );
  }, [open, agentConfig, agentConfigOptions]);

  // The dialog stays open through a gate change so its selections are kept,
  // but it submits only while publication is still the viewer's
  // (`g5.workspace.ui.reader-publication`).
  const publicationBlocked = usePublicationBlockedReason();

  if (!open) return null;

  const canSubmit =
    !!targetWorkspaceId &&
    !isSubmitting &&
    !!confirmedAgentLaunch &&
    publicationBlocked === null;

  return (
    <Modal
      onClose={onCancel}
      labelledBy="workspace-merge-confirm-title"
      size={targetWorkspaceId ? "xl" : "md"}
      closeDisabled={isSubmitting}
      testId="workspace-merge-confirm-modal"
      className={classNames(
        "flex flex-col !overflow-hidden p-0",
        targetWorkspaceId &&
          "h-[92vh] max-h-[92vh] max-w-[min(1500px,calc(100vw-2rem))]",
      )}
    >
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--g5-stroke)] px-4 py-3">
        <GitMerge size={17} className="text-[var(--g5-ok)]" />
        <h2
          id="workspace-merge-confirm-title"
          className="min-w-0 flex-1 truncate text-sm font-semibold text-[var(--g5-text0)]"
        >
          Rebase Workspace
        </h2>
        <button
          type="button"
          data-testid="workspace-merge-modal-close"
          onClick={onCancel}
          disabled={isSubmitting}
          className="flex h-7 w-7 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:opacity-50"
          title="Close"
        >
          <X size={15} />
        </button>
      </div>

      <div
        data-testid="workspace-merge-confirm-body"
        className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4"
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--g5-text2)]">
              Source workspace
            </label>
            <div className="truncate rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-2 text-sm text-[var(--g5-text0)]">
              {sourceWorkspaceName}
            </div>
          </div>
          <div>
            <label
              className="mb-1 block text-xs font-medium text-[var(--g5-text2)]"
              htmlFor="workspace-merge-target"
            >
              Target workspace
            </label>
            <ThemedSelect
              id="workspace-merge-target"
              testId="workspace-merge-target"
              value={targetWorkspaceId}
              onChange={onTargetWorkspaceChange}
              disabled={isSubmitting}
              options={targetOptions}
            />
          </div>
        </div>

        <section
          data-testid="rebase-execution-controls"
          aria-label="Rebase execution controls"
          className="space-y-3 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
        >
          <AgentLaunchConfigControls
            value={selectedAgentConfig}
            options={agentConfigOptions}
            onChange={handleAgentConfigChange}
            resolvePreferences={resolveAgentLaunch}
            onResolutionSelectionChange={setConfirmedAgentLaunch}
            disabled={isSubmitting}
            testIdPrefix="workspace-merge"
            selectVariant="themed"
          />
        </section>

        <ol className="grid gap-2 sm:grid-cols-2">
          {PHASES.map((phase, index) => (
            <li
              key={phase}
              data-testid="workspace-merge-confirm-phase"
              className="flex items-center gap-2 text-sm text-[var(--g5-text1)]"
            >
              <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-[var(--g5-stroke)] text-[11px] text-[var(--g5-text2)]">
                {index + 1}
              </span>
              <span className="truncate">{phase}</span>
            </li>
          ))}
        </ol>

        <div className="flex gap-2 rounded-md border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-2 text-xs text-[var(--g5-text1)]">
          <AlertTriangle
            size={15}
            className="mt-0.5 shrink-0 text-[var(--g5-warn)]"
          />
          <p>
            This rebases the current workspace onto the selected target
            workspace and does not advance the target workspace. Avoid editing
            the current workspace while it runs.
          </p>
        </div>

        {errorMessage && (
          <div className="rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]">
            {errorMessage}
          </div>
        )}

        {targetWorkspaceId && (
          <RebaseChangeGraphPreview
            sourceWorkspaceId={sourceWorkspaceId}
            targetWorkspaceId={targetWorkspaceId}
            sourceWorkspaceName={sourceWorkspaceName}
            targetWorkspaceName={targetWorkspaceName}
          />
        )}
      </div>

      <div
        data-testid="workspace-merge-modal-actions"
        className="flex shrink-0 items-center justify-end gap-2 border-t border-[var(--g5-stroke)] px-4 py-3"
      >
        {publicationBlocked !== null && (
          <p
            data-testid="workspace-merge-blocked"
            role="status"
            className="mr-auto text-xs text-[var(--g5-text2)]"
          >
            {publicationBlocked}
          </p>
        )}
        <button
          type="button"
          data-testid="workspace-merge-cancel"
          onClick={onCancel}
          disabled={isSubmitting}
          className="rounded-md px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={() => {
            if (!confirmedAgentLaunch || !canSubmit) return;
            onConfirm(selectedAgentConfig, confirmedAgentLaunch);
          }}
          disabled={!canSubmit}
          title={publicationBlocked ?? undefined}
          data-testid="workspace-merge-confirm-start"
          className={classNames(
            "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium text-white",
            canSubmit
              ? "bg-[var(--g5-ok)] hover:bg-[var(--g5-ok)]/90"
              : "cursor-not-allowed bg-[var(--g5-ok)]/60",
          )}
        >
          <GitMerge size={14} />
          {isSubmitting ? "Rebasing..." : "Rebase Workspace"}
        </button>
      </div>
    </Modal>
  );
}
