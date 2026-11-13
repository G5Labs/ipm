import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { RenameWorkspaceDialog } from "./RenameWorkspaceDialog";
import type { WorkspaceSummary } from "../../lib/types";

const workspace = {
  id: "ws-1",
  name: "foo",
  description: null,
  isBase: false,
  ownerUserId: "user-1",
  createdBy: "user-1",
  clonedFromId: null,
  branches: [],
  tasksCompleted: 0,
  totalTasks: 0,
  runningTime: null,
  cost: 0,
  concurrentTasks: 0,
  lastUpdated: "2026-10-23T00:00:00Z",
  createdAt: "2026-10-23T00:00:00Z",
} as unknown as WorkspaceSummary;

// Rename a workspace (g5.workspace.rename): prefilled with the current name,
// submits a changed name, stays open with the server's reason on refusal.
describe("RenameWorkspaceDialog", () => {
  it("submits a changed name and refuses an unchanged or blank one", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    render(
      <RenameWorkspaceDialog
        workspace={workspace}
        onClose={() => {}}
        onConfirm={onConfirm}
      />,
    );
    expect(screen.getByRole("heading", { name: "Rename foo" })).toBeVisible();
    expect(
      screen.getByText(/The Git branch keeps its current name/),
    ).toBeVisible();
    const input = screen.getByTestId("rename-workspace-input");
    expect(input).toHaveValue("foo");
    expect(screen.getByTestId("rename-workspace-confirm")).toBeDisabled();

    await user.clear(input);
    expect(screen.getByTestId("rename-workspace-confirm")).toBeDisabled();
    await user.type(input, "  bar  ");
    await user.click(screen.getByTestId("rename-workspace-confirm"));
    expect(onConfirm).toHaveBeenCalledWith("bar");
  });

  it("keeps the dialog open with the server's reason after a refused submit", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onConfirm = vi.fn().mockRejectedValue(new Error("refused"));
    render(
      <RenameWorkspaceDialog
        workspace={workspace}
        errorMessage="The name 'bar' is another workspace in this organization"
        onClose={onClose}
        onConfirm={onConfirm}
      />,
    );
    const input = screen.getByTestId("rename-workspace-input");
    await user.clear(input);
    await user.type(input, "bar");
    await user.click(screen.getByTestId("rename-workspace-confirm"));
    expect(onConfirm).toHaveBeenCalledWith("bar");
    expect(screen.getByTestId("rename-workspace-dialog")).toBeVisible();
    expect(screen.getByTestId("rename-workspace-error")).toHaveTextContent(
      "another workspace",
    );
    expect(onClose).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("rename-workspace-cancel"));
    expect(onClose).toHaveBeenCalled();
  });
});
