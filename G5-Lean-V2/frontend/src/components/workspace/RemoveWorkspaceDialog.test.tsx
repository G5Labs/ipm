import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { renderApp } from "../../test/render";
import { createMockWorkspace } from "../../test/fixtures";
import { RemoveWorkspaceDialog } from "./RemoveWorkspaceDialog";

describe("RemoveWorkspaceDialog", () => {
  it("renders the tombstone confirmation copy without preflight warnings", () => {
    renderApp(
      <RemoveWorkspaceDialog
        workspace={createMockWorkspace({
          id: "ws-delete",
          name: "Delete Me",
        })}
        onClose={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("heading", {
        name: "Are you sure you want to remove this workspace?",
      }),
    ).toBeVisible();
    expect(
      screen.getByText(
        "This removes the workspace from your org and reserves its name. It won't be listed anymore.",
      ),
    ).toBeVisible();
    expect(screen.getByText("Delete Me")).toBeVisible();
    expect(screen.queryByText(/unmerged commits/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/dirty files/i)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/checking workspace state/i),
    ).not.toBeInTheDocument();
  });

  it("requires the exact case-sensitive workspace name before confirming", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();

    renderApp(
      <RemoveWorkspaceDialog
        workspace={createMockWorkspace({
          id: "ws-delete",
          name: "Delete Me",
        })}
        onClose={vi.fn()}
        onConfirm={onConfirm}
      />,
    );

    const confirmButton = screen.getByRole("button", {
      name: /remove workspace/i,
    });
    const nameInput = screen.getByLabelText(/type the workspace name/i);

    expect(confirmButton).toBeDisabled();

    await user.type(nameInput, "delete me");
    expect(confirmButton).toBeDisabled();

    await user.clear(nameInput);
    await user.type(nameInput, "Delete Me");
    expect(confirmButton).toBeEnabled();

    await user.click(confirmButton);

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("resets typed confirmation when the loaded workspace changes", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onClose = vi.fn();

    const { rerender } = renderApp(
      <RemoveWorkspaceDialog
        workspace={createMockWorkspace({
          id: "ws-delete",
          name: "Cached Name",
        })}
        onClose={onClose}
        onConfirm={onConfirm}
      />,
    );

    const confirmButton = screen.getByRole("button", {
      name: /remove workspace/i,
    });
    const nameInput = screen.getByLabelText(/type the workspace name/i);

    expect(screen.getByText("Cached Name")).toBeVisible();

    await user.type(nameInput, "Cached Name");
    expect(confirmButton).toBeEnabled();

    rerender(
      <RemoveWorkspaceDialog
        workspace={createMockWorkspace({
          id: "ws-other",
          name: "Server Name",
        })}
        onClose={onClose}
        onConfirm={onConfirm}
      />,
    );

    expect(screen.getByText("Server Name")).toBeVisible();
    expect(screen.getByLabelText(/type the workspace name/i)).toHaveValue("");
    expect(
      screen.getByRole("button", { name: /remove workspace/i }),
    ).toBeDisabled();
  });

  it("keeps a refused removal in the dialog for a retry, without an escaping rejection", async () => {
    const user = userEvent.setup();
    const unhandled: unknown[] = [];
    const onUnhandled = (event: PromiseRejectionEvent) => {
      unhandled.push(event.reason);
      event.preventDefault();
    };
    window.addEventListener("unhandledrejection", onUnhandled);
    const onConfirm = vi
      .fn()
      .mockRejectedValueOnce(new Error("Workspace is stopping"))
      .mockResolvedValueOnce(undefined);
    try {
      renderApp(
        <RemoveWorkspaceDialog
          workspace={createMockWorkspace({
            id: "ws-delete",
            name: "Delete Me",
          })}
          onClose={vi.fn()}
          onConfirm={onConfirm}
        />,
      );
      await user.type(
        screen.getByLabelText(/type the workspace name/i),
        "Delete Me",
      );
      const confirmButton = screen.getByRole("button", {
        name: /remove workspace/i,
      });
      await user.click(confirmButton);
      expect(onConfirm).toHaveBeenCalledTimes(1);
      // The dialog is still there, with the typed name, for the retry.
      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(confirmButton).toBeEnabled();
      await user.click(confirmButton);
      expect(onConfirm).toHaveBeenCalledTimes(2);
      // Let a stray rejection reach the window before asserting none did.
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(unhandled).toEqual([]);
    } finally {
      window.removeEventListener("unhandledrejection", onUnhandled);
    }
  });

  it("shows a mutation error message when removal fails", () => {
    renderApp(
      <RemoveWorkspaceDialog
        workspace={createMockWorkspace({
          id: "ws-delete",
          name: "Delete Me",
        })}
        onClose={vi.fn()}
        onConfirm={vi.fn()}
        errorMessage="Removal failed"
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("Removal failed");
  });
});
