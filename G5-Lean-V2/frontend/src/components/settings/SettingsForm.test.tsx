// =============================================================================
// SettingsForm tests — the owner's opt-in for the workspace agent to change
// other workspaces (g5.security.agent-other-workspaces).
// =============================================================================

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { WorkspaceSettings } from "../../lib/types";
import { SettingsForm } from "./SettingsForm";

const settings: WorkspaceSettings = {
  workspaceId: "ws-1",
  maxSpendUsd: null,
  spendPeriod: "monthly",
  spendPeriodStart: null,
  maxInputTokens: null,
  maxOutputTokens: null,
  idleTimeoutSecs: 28800,
  responseDeadlineSecs: 900,
  boundedExecutionSlots: 16,
  permissionPolicy: null,
  agentMayModifyOtherWorkspaces: false,
  updatedBy: null,
  updatedAt: "2026-10-23T00:00:00Z",
};

describe("SettingsForm — agent may modify other workspaces", () => {
  it("shows the stored value and saves only the changed opt-in", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsForm
        settings={settings}
        onSave={onSave}
        saving={false}
        errors={{}}
      />,
    );
    const optIn = screen.getByRole("checkbox", {
      name: /Agent may modify other workspaces/,
    });
    expect(optIn).not.toBeChecked();
    expect(screen.getByTestId("settings-form-save")).toBeDisabled();

    await userEvent.click(optIn);
    expect(optIn).toBeChecked();
    await userEvent.click(screen.getByTestId("settings-form-save"));
    expect(onSave).toHaveBeenCalledWith({
      agentMayModifyOtherWorkspaces: true,
    });
  });
});
