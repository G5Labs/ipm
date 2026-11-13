import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import { describe, expect, it, vi } from "vitest";

import type {
  AgentLaunchPreferencesV1,
  ResolvedAgentExecutionV1,
  SyncAgentConfig,
} from "../../lib/types";
import { WorkspaceMergeConfirmModal } from "./WorkspaceMergeConfirmModal";

vi.mock("./RebaseChangeGraphPreview", () => ({
  RebaseChangeGraphPreview: () => (
    <div data-testid="rebase-change-graph-preview">Graph preview</div>
  ),
}));

const AGENT_CONFIG: SyncAgentConfig = {
  agentProvider: "claude_code",
  permissionMode: "bypassPermissions",
  permissionLabel: "Bypass",
  model: undefined,
  modelLabel: "Opus 4.8 [1M]",
};

const resolveAgentLaunch = vi.fn(
  async (
    preferences: AgentLaunchPreferencesV1,
  ): Promise<ResolvedAgentExecutionV1> => ({
    schemaVersion: 1,
    resolutionId: `alr_${"a".repeat(64)}`,
    configRevision: "config-1",
    catalogVersion: "catalog-1",
    provider: { value: "claude_code", provenance: "configured" },
    providerRoute: "anthropic_direct",
    model: { value: "claude-opus-4-8[1m]", provenance: "configured" },
    upstreamModel: "anthropic/claude-opus-4-8[1m]",
    permissionMode: {
      value: "bypassPermissions",
      provenance: "configured",
    },
    effort: {
      value:
        preferences.effort.source === "explicit"
          ? preferences.effort.value
          : "high",
      provenance: "configured",
    },
    authMode: "anthropic_api_key",
    egressMode: "direct_provider",
    credentialReady: true,
    requiresInteractiveAuth: false,
  }),
);

function renderModal(onConfirm = vi.fn()) {
  render(
    <WorkspaceMergeConfirmModal
      open
      sourceWorkspaceId="source-1"
      sourceWorkspaceName="feature"
      targets={[{ id: "target-1", name: "main" }]}
      targetWorkspaceId="target-1"
      agentConfig={AGENT_CONFIG}
      resolveAgentLaunch={resolveAgentLaunch}
      onTargetWorkspaceChange={vi.fn()}
      onCancel={vi.fn()}
      onConfirm={onConfirm}
    />,
  );
  return onConfirm;
}

describe("WorkspaceMergeConfirmModal", () => {
  it("disables Start once the source becomes read-only, keeping the selections and calling nothing (g5.workspace.ui.reader-publication)", async () => {
    const onConfirm = vi.fn();
    const gate = (writable: boolean) => ({
      kind: writable ? ("live" as const) : ("read_only" as const),
      canWrite: writable,
      live: writable,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: true,
      workspaceNoun: "the workspace",
    });
    function Host() {
      const [writable, setWritable] = useState(true);
      return (
        <WorkspaceLiveGateProvider value={gate(writable)}>
          <button
            type="button"
            data-testid="make-read-only"
            onClick={() => setWritable(false)}
          >
            freeze
          </button>
          <WorkspaceMergeConfirmModal
            open
            sourceWorkspaceId="source-1"
            sourceWorkspaceName="feature"
            targets={[{ id: "target-1", name: "main" }]}
            targetWorkspaceId="target-1"
            agentConfig={AGENT_CONFIG}
            resolveAgentLaunch={resolveAgentLaunch}
            onTargetWorkspaceChange={vi.fn()}
            onCancel={vi.fn()}
            onConfirm={onConfirm}
          />
        </WorkspaceLiveGateProvider>
      );
    }
    render(<Host />);
    const start = screen.getByTestId("workspace-merge-confirm-start");
    await waitFor(() => expect(start).toBeEnabled());
    expect(screen.queryByTestId("workspace-merge-blocked")).toBeNull();

    fireEvent.click(screen.getByTestId("make-read-only"));

    await waitFor(() => expect(start).toBeDisabled());
    expect(screen.getByTestId("workspace-merge-blocked")).toHaveTextContent(
      "This workspace is read-only; make it editable in Workspace Details to publish.",
    );
    fireEvent.click(start);
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByTestId("workspace-merge-confirm-body")).toBeVisible();
  });

  it("keeps the actions outside the scrolling content", () => {
    renderModal();
    expect(
      screen.getByTestId("workspace-merge-confirm-body"),
    ).not.toContainElement(screen.getByTestId("workspace-merge-modal-actions"));
  });

  it("starts the workspace rebase with the resolved agent configuration", async () => {
    const onConfirm = renderModal();
    await waitFor(() =>
      expect(screen.getByTestId("workspace-merge-confirm-start")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("workspace-merge-confirm-start"));
    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({ agentProvider: "claude_code" }),
      expect.objectContaining({
        confirmedResolutionId: `alr_${"a".repeat(64)}`,
      }),
    );
  });

  it("shows the target change graph and rebase warning", () => {
    renderModal();
    expect(screen.getByTestId("rebase-change-graph-preview")).toBeVisible();
    expect(
      screen.getByText(
        /rebases the current workspace onto the selected target/i,
      ),
    ).toBeVisible();
  });
});
