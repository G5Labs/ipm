import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceNamesProvider } from "../../hooks/useWorkspaceNames";
import type { AsyncState, BranchSummary, FileTreeEntry } from "../../lib/types";
import { renderApp } from "../../test/render";
import { WorkspaceFileTree } from "./WorkspaceFileTree";

const { mockUseFileTree } = vi.hoisted(() => ({
  mockUseFileTree: vi.fn(),
}));

vi.mock("../../hooks/useFiles", () => ({
  // The component reads the labeled tree; tests may answer with plain
  // entries (an unlabeled read) or a full read.
  useFileTreeRead: (...args: unknown[]) => {
    const state = mockUseFileTree(...args);
    if (state?.status === "ok" && Array.isArray(state.data)) {
      return {
        status: "ok",
        data: {
          entries: state.data,
          truncated: false,
          source: null,
          repoOid: null,
          snapshotAt: null,
          liveFallback: null,
        },
      };
    }
    return state;
  },
}));

function branch(overrides: Partial<BranchSummary>): BranchSummary {
  return {
    workspaceId: "workspace-1",

    repoId: "repo-code",
    repoName: "code",
    branchName: "feature/code",

    trackingMode: "tracking",
    headCommit: "a".repeat(40),
    divergence: 0,
    behindBase: 0,
    hasDraft: false,
    dirtyFileCount: 0,
    ...overrides,
  };
}

const codeBranch = branch({});
const docsBranch = branch({
  workspaceId: "workspace-1",

  repoId: "repo-docs",
  repoName: "docs",
  branchName: "docs-main",
  headCommit: "b".repeat(40),
});

function rootContainer(branchId: string): HTMLElement {
  const row = document.querySelector(
    `[role="treeitem"][data-g5-node-id="${branchId}"]`,
  );
  if (!row?.parentElement) {
    throw new Error(`Repository root ${branchId} missing`);
  }
  return row.parentElement;
}

function renderTree(
  overrides: Partial<ComponentProps<typeof WorkspaceFileTree>> = {},
  workspaceNamesById: Record<string, string> = {},
) {
  return renderApp(
    <WorkspaceNamesProvider value={workspaceNamesById}>
      <WorkspaceFileTree
        branches={[codeBranch, docsBranch]}
        organization="g5labs"
        selectedFile={null}
        onSelectFile={vi.fn()}
        {...overrides}
      />
    </WorkspaceNamesProvider>,
  );
}

describe("WorkspaceFileTree", () => {
  beforeEach(() => {
    mockUseFileTree.mockReset();
    mockUseFileTree.mockReturnValue({ status: "ok", data: [] });
  });

  it("renders code and docs repository roots collapsed with unqualified repository labels", () => {
    renderTree({
      selectedFile: { branchId: docsBranch.repoId, path: "README.md" },
    });

    expect(screen.getAllByRole("tree")).toHaveLength(1);
    expect(
      screen.getByRole("tree", { name: "Workspace repository files" }),
    ).toBeInTheDocument();
    expect(screen.getByText("code")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(screen.queryByText("g5labs/code")).not.toBeInTheDocument();
    expect(screen.queryByText("g5labs/docs")).not.toBeInTheDocument();
    const codeLabel = screen.getByText("code");
    const codeRoot = codeLabel.closest('[role="treeitem"]');
    const repositoryIcon = codeRoot?.querySelector(".lucide-folder");
    expect(codeLabel).toHaveClass("text-[var(--g5-info)]");
    expect(repositoryIcon).toHaveClass("text-[var(--g5-info)]");
    expect(
      codeRoot?.querySelector(".lucide-git-branch"),
    ).not.toBeInTheDocument();
    expect(
      codeRoot?.querySelector(".lucide-folder-root"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("feature/code")).not.toBeInTheDocument();
    expect(screen.queryByText("docs-main")).not.toBeInTheDocument();
    expect(
      screen.getByText("code").closest('[role="treeitem"]'),
    ).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.getByText("docs").closest('[role="treeitem"]'),
    ).toHaveAttribute("aria-expanded", "false");
    expect(mockUseFileTree).not.toHaveBeenCalled();
  });

  it("uses the unqualified provider repository name for remote-tracking roots", () => {
    const remoteCodeBranch = branch({
      trackingMode: "remote_tracking",
      remoteRepoName: "G5Labs/G5",
      remoteBranch: "feature/repository-files",
    });
    renderTree({ branches: [remoteCodeBranch, docsBranch] });

    expect(screen.getByText("G5")).toBeInTheDocument();
    expect(screen.queryByText("G5Labs/G5")).not.toBeInTheDocument();
    expect(screen.queryByText("g5labs/code")).not.toBeInTheDocument();
    expect(
      screen.queryByText("feature/repository-files"),
    ).not.toBeInTheDocument();
  });

  it("loads only expanded branches and isolates loading and error states", async () => {
    const states: Record<string, AsyncState<FileTreeEntry[]>> = {
      [codeBranch.repoId]: { status: "loading" },
      [docsBranch.repoId]: {
        status: "error",
        error: new Error("docs unavailable"),
      },
    };
    mockUseFileTree.mockImplementation(
      (_workspaceId: string, branchId: string) => states[branchId],
    );
    const user = userEvent.setup();
    renderTree();

    await user.click(screen.getByText("code"));
    expect(
      screen
        .getByText("code")
        .closest('[role="treeitem"]')
        ?.querySelector(".lucide-folder-open"),
    ).toHaveClass("text-[var(--g5-info)]");
    expect(mockUseFileTree.mock.calls.map(([, branchId]) => branchId)).toEqual([
      codeBranch.repoId,
    ]);
    expect(
      within(rootContainer(codeBranch.repoId)).getByText("Loading files…"),
    ).toBeInTheDocument();
    expect(
      within(rootContainer(docsBranch.repoId)).queryByText(/docs unavailable/),
    ).not.toBeInTheDocument();

    await user.click(screen.getByText("docs"));
    expect(
      mockUseFileTree.mock.calls.map(([, branchId]) => branchId),
    ).toContain(docsBranch.repoId);
    expect(
      within(rootContainer(docsBranch.repoId)).getByText(
        "Could not load files: docs unavailable",
      ),
    ).toBeInTheDocument();
    expect(
      within(rootContainer(codeBranch.repoId)).getByText("Loading files…"),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("tree")).toHaveLength(1);
  });

  it("says when the workspace runtime predates the live listing", async () => {
    mockUseFileTree.mockReturnValue({
      status: "ok",
      data: {
        entries: [],
        truncated: false,
        source: "committed",
        repoOid: "a".repeat(40),
        snapshotAt: "2026-10-05T00:00:00Z",
        liveFallback: "runtime_outdated",
      },
    });
    const user = userEvent.setup();
    renderTree({ branches: [codeBranch] });

    await user.click(screen.getByText("code"));
    expect(
      within(rootContainer(codeBranch.repoId)).getByTestId(
        "workspace-file-tree-source",
      ),
    ).toHaveTextContent(
      "Showing committed files: restart the workspace to list the live checkout.",
    );
  });

  it("says when the durable cut replaced the owner's live checkout", async () => {
    mockUseFileTree.mockReturnValue({
      status: "ok",
      data: {
        entries: [
          {
            name: "README.md",
            path: "README.md",
            type: "file",
            size: 12,
          },
        ],
        truncated: false,
        source: "committed",
        repoOid: "a".repeat(40),
        snapshotAt: "2026-10-05T00:00:00Z",
        liveFallback: "entry_cap",
      },
    });
    const user = userEvent.setup();
    renderTree({ branches: [codeBranch] });

    await user.click(screen.getByText("code"));
    const root = rootContainer(codeBranch.repoId);
    expect(
      within(root).getByTestId("workspace-file-tree-source"),
    ).toHaveTextContent(
      "Showing committed files: the live checkout has too many files to list.",
    );
    expect(within(root).getByText("README.md")).toBeInTheDocument();
    expect(
      within(root).queryByTestId("workspace-file-tree-truncated"),
    ).not.toBeInTheDocument();
  });

  it("says when the committed listing stopped at the tree's cap", async () => {
    mockUseFileTree.mockReturnValue({
      status: "ok",
      data: {
        entries: [
          {
            name: "README.md",
            path: "README.md",
            type: "file",
            size: 12,
          },
        ],
        truncated: true,
        source: "committed",
        repoOid: "a".repeat(40),
        snapshotAt: "2026-10-05T00:00:00Z",
        liveFallback: null,
      },
    });
    const user = userEvent.setup();
    renderTree({ branches: [codeBranch] });

    await user.click(screen.getByText("code"));
    const root = rootContainer(codeBranch.repoId);
    expect(
      within(root).getByTestId("workspace-file-tree-truncated"),
    ).toHaveTextContent(
      "Showing the first files only: this folder has too many files to list.",
    );
    expect(
      within(root).queryByTestId("workspace-file-tree-source"),
    ).not.toBeInTheDocument();
    expect(within(root).getByText("README.md")).toBeInTheDocument();
  });

  it("keeps empty and draft-overlay results inside their owning repository", async () => {
    mockUseFileTree.mockImplementation(
      (_workspaceId: string, branchId: string) =>
        branchId === codeBranch.repoId
          ? { status: "ok", data: [] }
          : {
              status: "ok",
              data: [
                {
                  name: "draft-only.md",
                  path: "draft-only.md",
                  type: "file",
                },
              ],
            },
    );
    const user = userEvent.setup();
    renderTree();

    await user.click(screen.getByText("code"));
    await user.click(screen.getByText("docs"));

    expect(
      within(rootContainer(codeBranch.repoId)).getByText("No files found"),
    ).toBeInTheDocument();
    expect(
      within(rootContainer(codeBranch.repoId)).queryByText("draft-only.md"),
    ).not.toBeInTheDocument();
    expect(
      within(rootContainer(docsBranch.repoId)).getByText("draft-only.md"),
    ).toBeInTheDocument();
  });

  it("labels a docs workspace folder by name while preserving its canonical path identity", async () => {
    const workspaceId = "019f66e6-1334-7292-98ef-a1234567890b";
    const filePath = `docs/${workspaceId}/roadmap.md`;
    const docsTree: FileTreeEntry[] = [
      {
        name: "docs",
        path: "docs",
        type: "directory",
        size: null,
        children: [
          {
            name: workspaceId,
            path: `docs/${workspaceId}`,
            type: "directory",
            size: null,
            children: [
              {
                name: "roadmap.md",
                path: filePath,
                type: "file",
                size: 1,
              },
            ],
          },
        ],
      },
    ];
    mockUseFileTree.mockImplementation(
      (_workspaceId: string, branchId: string) => ({
        status: "ok",
        data: branchId === docsBranch.repoId ? docsTree : [],
      }),
    );
    const onSelectFile = vi.fn();
    const onCreateFolder = vi.fn();
    const user = userEvent.setup();
    renderTree(
      { onSelectFile, onCreateFolder },
      { [workspaceId]: "Compliance Workspace" },
    );

    await user.click(screen.getByText("docs"));
    const docsRoot = rootContainer(docsBranch.repoId);
    const docsFolderLabel = within(docsRoot).getAllByText("docs", {
      exact: true,
    })[1];
    if (!docsFolderLabel) throw new Error("Nested docs folder label missing");
    await user.click(docsFolderLabel);

    expect(
      within(docsRoot).getByText("Compliance Workspace", { exact: true }),
    ).toBeInTheDocument();
    expect(within(docsRoot).queryByText(workspaceId)).not.toBeInTheDocument();

    await user.click(
      within(docsRoot).getByText("Compliance Workspace", { exact: true }),
    );
    const fileRow = within(docsRoot).getByText("roadmap.md", { exact: true });
    await user.click(fileRow);
    expect(onSelectFile).toHaveBeenLastCalledWith(docsBranch.repoId, filePath);

    fireEvent.contextMenu(fileRow);
    await user.click(screen.getByRole("menuitem", { name: "New Folder" }));
    expect(onCreateFolder).toHaveBeenLastCalledWith(
      docsBranch.repoId,
      `docs/${workspaceId}`,
    );
  });

  it("keeps identical paths independently expanded and binds actions to the owning branch", async () => {
    const sharedTree: FileTreeEntry[] = [
      {
        name: "src",
        path: "src",
        type: "directory",
        size: null,
        children: [
          {
            name: "shared.ts",
            path: "src/shared.ts",
            type: "file",
            size: 1,
          },
        ],
      },
    ];
    mockUseFileTree.mockReturnValue({ status: "ok", data: sharedTree });
    const onSelectFile = vi.fn();
    const onCreateFile = vi.fn();
    const onCreateFolder = vi.fn();
    const onDeleteEntry = vi.fn();
    const user = userEvent.setup();
    renderTree({
      onSelectFile,
      onCreateFile,
      onCreateFolder,
      onDeleteEntry,
    });

    await user.click(screen.getByText("code"));
    await user.click(screen.getByText("docs"));
    const codeRoot = rootContainer(codeBranch.repoId);
    const docsRoot = rootContainer(docsBranch.repoId);

    await user.click(within(codeRoot).getByText("src"));
    expect(within(codeRoot).getByText("shared.ts")).toBeInTheDocument();
    expect(within(docsRoot).queryByText("shared.ts")).not.toBeInTheDocument();

    await user.click(within(codeRoot).getByText("shared.ts"));
    expect(onSelectFile).toHaveBeenLastCalledWith(
      codeBranch.repoId,
      "src/shared.ts",
    );

    fireEvent.contextMenu(within(codeRoot).getByText("shared.ts"));
    await user.click(screen.getByRole("menuitem", { name: "New File" }));
    expect(onCreateFile).toHaveBeenLastCalledWith(codeBranch.repoId, "src");

    fireEvent.contextMenu(within(codeRoot).getByText("shared.ts"));
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(onDeleteEntry).toHaveBeenLastCalledWith(
      codeBranch.repoId,
      expect.objectContaining({ path: "src/shared.ts" }),
    );

    await user.click(within(docsRoot).getByText("src"));
    expect(within(docsRoot).getByText("shared.ts")).toBeInTheDocument();
    await user.click(within(docsRoot).getByText("shared.ts"));
    expect(onSelectFile).toHaveBeenLastCalledWith(
      docsBranch.repoId,
      "src/shared.ts",
    );

    fireEvent.contextMenu(within(docsRoot).getByText("shared.ts"));
    await user.click(screen.getByRole("menuitem", { name: "New Folder" }));
    expect(onCreateFolder).toHaveBeenLastCalledWith(docsBranch.repoId, "src");
  });
});
