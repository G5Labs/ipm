import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { FileTree } from "./FileTree";

describe("FileTree", () => {
  it("shows right-click create and delete actions for files and folders", async () => {
    const onCreateFile = vi.fn();
    const onCreateFolder = vi.fn();
    const onDeleteEntry = vi.fn();

    renderApp(
      <FileTree
        entries={[
          {
            name: "src",
            path: "src",
            type: "directory",
            children: [
              {
                name: "main.rs",
                path: "src/main.rs",
                type: "file",
              },
            ],
          },
        ]}
        selectedPath={null}
        expandedPaths={new Set(["src"])}
        onSelectFile={vi.fn()}
        onToggleExpand={vi.fn()}
        onCreateFile={onCreateFile}
        onCreateFolder={onCreateFolder}
        onDeleteEntry={onDeleteEntry}
      />,
    );

    fireEvent.contextMenu(screen.getByText("main.rs"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New File" }));
    expect(onCreateFile).toHaveBeenCalledWith("src");

    fireEvent.contextMenu(screen.getByText("main.rs"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Folder" }));
    expect(onCreateFolder).toHaveBeenCalledWith("src");

    fireEvent.contextMenu(screen.getByText("main.rs"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(onDeleteEntry).toHaveBeenCalledWith(
      expect.objectContaining({ path: "src/main.rs", type: "file" }),
    );

    fireEvent.contextMenu(screen.getByText("src"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(onDeleteEntry).toHaveBeenCalledWith(
      expect.objectContaining({ path: "src", type: "directory" }),
    );
  });

  // Issue #1066 MVP v1 (left-sidebar spec D5/D17): the durable `.g5` subtree
  // served by the API renders through the standard rows and standard
  // mutation affordances; write-blocked paths (runtime `.g5/**`, `.git/**`)
  // are never in the entries, so no affordance exists for them.
  it("renders the durable .g5 subtree with standard open and mutation affordances", async () => {
    const onSelectFile = vi.fn();
    const onDeleteEntry = vi.fn();
    const onCreateFile = vi.fn();

    renderApp(
      <FileTree
        entries={[
          {
            name: ".g5",
            path: ".g5",
            type: "directory",
            children: [
              {
                name: "workspace.yaml",
                path: ".g5/workspace.yaml",
                type: "file",
              },
              {
                name: "preview_profiles",
                path: ".g5/preview_profiles",
                type: "directory",
                children: [
                  {
                    name: "browser.yaml",
                    path: ".g5/preview_profiles/browser.yaml",
                    type: "file",
                  },
                ],
              },
            ],
          },
          { name: "README.md", path: "README.md", type: "file" },
        ]}
        selectedPath={null}
        expandedPaths={new Set([".g5", ".g5/preview_profiles"])}
        onSelectFile={onSelectFile}
        onToggleExpand={vi.fn()}
        onCreateFile={onCreateFile}
        onCreateFolder={vi.fn()}
        onDeleteEntry={onDeleteEntry}
      />,
    );

    // Durable .g5 rows render like any other rows.
    const g5Row = screen.getByText(".g5").closest('[role="treeitem"]');
    expect(g5Row).not.toBeNull();
    expect(g5Row?.querySelector(".lucide-folder-open")).not.toBeNull();
    expect(g5Row?.querySelector(".lucide-folder-tree")).toBeNull();
    expect(screen.queryByText("g5")).not.toBeInTheDocument();
    expect(screen.getByText("workspace.yaml")).toBeInTheDocument();
    expect(screen.getByText("browser.yaml")).toBeInTheDocument();

    // Opening a durable row goes through the standard file-select flow.
    await userEvent.click(screen.getByText("workspace.yaml"));
    expect(onSelectFile).toHaveBeenCalledWith(".g5/workspace.yaml");

    // Standard mutation affordances apply to durable rows.
    fireEvent.contextMenu(screen.getByText("browser.yaml"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await waitFor(() => {
      expect(onDeleteEntry).toHaveBeenCalledWith(
        expect.objectContaining({
          path: ".g5/preview_profiles/browser.yaml",
          type: "file",
        }),
      );
      expect(
        screen.queryByRole("menuitem", { name: "Delete" }),
      ).not.toBeInTheDocument();
    });
    fireEvent.contextMenu(screen.getByText("workspace.yaml"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New File" }));
    expect(onCreateFile).toHaveBeenCalledWith(".g5");

    // Write-blocked paths are absent from the server-provided entries, so no
    // row (and therefore no mutation affordance) exists for them.
    expect(screen.queryByText("auth_state.json")).not.toBeInTheDocument();
    expect(screen.queryByText(".git")).not.toBeInTheDocument();
  });

  it("shows root create actions when the tree is empty", async () => {
    const onCreateFile = vi.fn();
    const onCreateFolder = vi.fn();

    renderApp(
      <FileTree
        entries={[]}
        selectedPath={null}
        expandedPaths={new Set()}
        onSelectFile={vi.fn()}
        onToggleExpand={vi.fn()}
        onCreateFile={onCreateFile}
        onCreateFolder={onCreateFolder}
      />,
    );

    fireEvent.contextMenu(screen.getByText("No files found"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New File" }));
    expect(onCreateFile).toHaveBeenCalledWith(null);

    fireEvent.contextMenu(screen.getByText("No files found"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Folder" }));
    expect(onCreateFolder).toHaveBeenCalledWith(null);
  });
});
