// =============================================================================
// DeleteProjectDocDialog — the open confirmation follows the workspace's live
// gate (`g5.workspace.ui.unavailable`): a stop after it opened disables Delete
// with the state's reason and Start, keeps the confirmation open, and calls
// nothing.
// =============================================================================

import React from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import { DeleteProjectDocDialog } from "./DeleteProjectDocDialog";

const gate = (live: boolean) => ({
  kind: live ? ("live" as const) : ("stopped" as const),
  canWrite: true,
  live,
  canStart: !live,
  reviewCopy: false,
  reviewIdentityKnown: true,
  owner: true,
  workspaceNoun: "the workspace",
});

function Host({
  onConfirm,
  onCancel,
}: {
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const [live, setLive] = React.useState(true);
  return (
    <WorkspaceLiveGateProvider value={gate(live)}>
      <button
        type="button"
        data-testid="stop-workspace"
        onClick={() => setLive(false)}
      >
        stop
      </button>
      <DeleteProjectDocDialog
        name="auth-review.md"
        onCancel={onCancel}
        onConfirm={onConfirm}
      />
    </WorkspaceLiveGateProvider>
  );
}

describe("DeleteProjectDocDialog", () => {
  it("confirms while live", () => {
    const onConfirm = vi.fn();
    render(<Host onConfirm={onConfirm} onCancel={vi.fn()} />);
    expect(screen.getByText(/Delete "auth-review.md"\?/)).toBeInTheDocument();
    expect(screen.queryByTestId("project-doc-delete-start-hint")).toBeNull();
    fireEvent.click(screen.getByTestId("project-doc-delete-confirm"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("disables Delete with the state's reason and Start once the workspace stops, keeping the confirmation open", async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<Host onConfirm={onConfirm} onCancel={onCancel} />);
    const confirm = screen.getByTestId("project-doc-delete-confirm");
    expect(confirm).toBeEnabled();

    fireEvent.click(screen.getByTestId("stop-workspace"));

    await waitFor(() => expect(confirm).toBeDisabled());
    expect(confirm).toHaveAttribute(
      "title",
      "Start the workspace to delete documents.",
    );
    expect(screen.queryByTestId("project-doc-delete-start-hint")).toBeNull();
    fireEvent.click(
      screen.getByTestId("project-doc-delete-start-hint-trigger"),
    );
    expect(
      screen.getByTestId("project-doc-delete-start-hint"),
    ).toHaveTextContent("Start the workspace to delete documents.");
    expect(
      screen.getByTestId("project-doc-delete-start-hint-start"),
    ).toBeEnabled();
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
    expect(screen.getByText(/Delete "auth-review.md"\?/)).toBeInTheDocument();
  });
});
