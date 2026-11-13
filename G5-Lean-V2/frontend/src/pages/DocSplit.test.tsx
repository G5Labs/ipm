// =============================================================================
// Doc Split & Sync Integration Tests
// =============================================================================
// Tests the Project Docs / All Docs split (explainers live in Project Docs),
// section badge policy,
// INTERNAL_WS_PREFIX filtering, and sync-path behavior expectations.
// =============================================================================

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderApp } from "../test/render";
import { activateSidebarSection } from "../test/sidebar";

import { LeftSidebar, INTERNAL_WS_PREFIX } from "../components/tree/TreeNode";
import { WorkspaceNamesProvider } from "../hooks/useWorkspaceNames";
import { MINIMAL_WORKSPACE_UI_CAPABILITIES } from "../lib/capabilities";
import type { DocNode } from "../lib/types";

// ---------------------------------------------------------------------------
// Factory: Create a DocNode with sensible defaults
// ---------------------------------------------------------------------------

let _docSeq = 0;
function createMockDoc(overrides: Partial<DocNode> = {}): DocNode {
  _docSeq += 1;
  const id = overrides.id ?? `doc-test-${_docSeq}`;
  return {
    id,
    name: overrides.name ?? `Test Doc ${_docSeq}`,
    docType: overrides.docType ?? "spec",
    filePath: overrides.filePath ?? null,
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2025-01-01T00:00:00Z",
    createdAt: "2025-01-01T00:00:00Z",
    updatedAt: "2025-01-01T00:00:00Z",
    title: overrides.title ?? `Test Doc ${_docSeq}`,
    category: overrides.category ?? "spec",
    preview: overrides.preview ?? "preview text",
    content: overrides.content ?? "# Test",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Default LeftSidebar props factory
// ---------------------------------------------------------------------------

function defaultSidebarProps(overrides: Record<string, unknown> = {}) {
  return {
    isOpen: true,
    width: 280,
    workspaceId: "test-workspace",
    organization: "g5labs",
    branches: [],
    ontologyRoots: [],
    tasks: [],
    docs: [] as DocNode[],
    selectedNodeId: null as string | null,
    selectedDocId: null as string | null,
    selectedTaskId: null as string | null,
    onSelectNode: vi.fn(),
    onSelectDoc: vi.fn(),
    onSelectFile: vi.fn(),
    onSelectTask: vi.fn(),
    onLoadChildren: vi.fn().mockResolvedValue([]),
    searchQuery: "",
    ...overrides,
  };
}

function SidebarDocRefetchHarness({
  docs,
  overrides = {},
}: {
  docs: DocNode[];
  overrides?: Record<string, unknown>;
}) {
  const [currentDocs, setCurrentDocs] = useState(docs);

  return (
    <>
      <button
        type="button"
        onClick={() =>
          setCurrentDocs((prev) => prev.map((doc) => ({ ...doc })))
        }
      >
        Refetch docs
      </button>
      <LeftSidebar
        {...defaultSidebarProps({ docs: currentDocs, ...overrides })}
      />
    </>
  );
}

function docFolderButton(
  container: HTMLElement,
  name: string,
): HTMLButtonElement {
  const button = within(container).getByText(name).closest("button");
  if (!button) throw new Error(`${name} folder not found`);
  return button as HTMLButtonElement;
}

function docFolderLabels(container: HTMLElement): string[] {
  return within(container)
    .queryAllByTestId("doc-folder")
    .map((button) => button.textContent ?? "");
}

/**
 * Expand every folder in a docs section, depth by depth. Used where two folders
 * share a display name (several workspaces each own an `explainers/` subfolder),
 * so `docFolderButton`'s by-name lookup would be ambiguous.
 */
async function expandAllDocFolders(container: HTMLElement): Promise<void> {
  for (let depth = 0; depth < 8; depth += 1) {
    const collapsed = within(container)
      .queryAllByTestId("doc-folder")
      .filter((button) => button.getAttribute("aria-expanded") === "false");
    if (collapsed.length === 0) return;
    for (const button of collapsed) {
      await userEvent.click(button);
      // Each click re-renders the section; settle this folder before finding
      // and clicking the next one.
      await waitFor(() =>
        expect(button).toHaveAttribute("aria-expanded", "true"),
      );
    }
  }
}

function fileDataTransfer(files: File[], types: string[] = ["Files"]) {
  return { files, types, dropEffect: "none" };
}

// ---------------------------------------------------------------------------
// Reset
// ---------------------------------------------------------------------------

beforeEach(() => {
  _docSeq = 0;
});

// =============================================================================
// Test Suite: INTERNAL_WS_PREFIX constant
// =============================================================================

describe("INTERNAL_WS_PREFIX", () => {
  it("is the expected prefix string", () => {
    expect(INTERNAL_WS_PREFIX).toBe("#!internal-workspace-");
  });

  it("can be used to match internal workspace folders", () => {
    expect(
      "#!internal-workspace-demo/README.md".startsWith(INTERNAL_WS_PREFIX),
    ).toBe(true);
    expect("Architecture".startsWith(INTERNAL_WS_PREFIX)).toBe(false);
    expect("".startsWith(INTERNAL_WS_PREFIX)).toBe(false);
  });
});

// =============================================================================
// Test Suite: docs sections split
// =============================================================================

describe("Project Docs vs All Docs split", () => {
  // Explainers are Project Docs under the workspace's `explainers/` subfolder
  // (owner instruction, 2026-10-30): no rail section of their own. These
  // fixtures live on the current workspace's path.
  const WS_FOLDER = "docs/test-workspace";

  it("lists explainer docs inside Project Docs under explainers/ and opens them by identity", async () => {
    const user = userEvent.setup();
    const onSelectDoc = vi.fn();
    const explainerDoc = createMockDoc({
      id: "doc-explainer-open",
      title: "Auth Flow Explainer",
      name: "auth-flow-2026-08-06-1200.html",
      docType: "explainer",
      category: "explainer",
      filePath: `${WS_FOLDER}/explainers/auth-flow-2026-08-06-1200.html`,
    });
    const props = defaultSidebarProps({
      docs: [explainerDoc],
      docsFolderPath: WS_FOLDER,
      onSelectDoc,
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    // No Explainers section, header or rail row anywhere.
    expect(screen.queryByTestId("sidebar-explainers-tab")).toBeNull();
    expect(screen.queryByText("Explainers")).toBeNull();

    activateSidebarSection("sidebar-docs-tab");
    const docsBlock = screen.getByTestId("sidebar-docs-tab");
    await waitFor(() => {
      expect(within(docsBlock).getByText("explainers")).toBeInTheDocument();
    });
    await user.click(docFolderButton(docsBlock, "explainers"));
    await waitFor(() => {
      expect(
        within(docsBlock).getByText("auth-flow-2026-08-06-1200.html"),
      ).toBeInTheDocument();
    });

    await user.click(
      within(docsBlock).getByText("auth-flow-2026-08-06-1200.html"),
    );
    expect(onSelectDoc).toHaveBeenCalledTimes(1);
    expect(onSelectDoc).toHaveBeenCalledWith("doc-explainer-open");
  });

  it("folds view docs into the All Docs listing", async () => {
    const user = userEvent.setup();
    const onSelectDoc = vi.fn();
    const viewDoc = createMockDoc({
      id: "doc-folded-view",
      title: "Billing PM View",
      name: "billing.pm_view.md",
      docType: "pm_view",
      category: "pm_view",
      filePath: "views/pm_views/billing.pm_view.md",
    });
    const props = defaultSidebarProps({ docs: [viewDoc], onSelectDoc });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-all-docs-tab");

    const allDocsBlock = screen.getByTestId("sidebar-all-docs-tab");
    await waitFor(() => {
      expect(within(allDocsBlock).getByText("views")).toBeInTheDocument();
    });
    await user.click(docFolderButton(allDocsBlock, "views"));
    await user.click(docFolderButton(allDocsBlock, "pm_views"));
    await waitFor(() => {
      expect(
        within(allDocsBlock).getByText("billing.pm_view.md"),
      ).toBeInTheDocument();
    });

    // The view row uses the shared opener.
    await user.click(within(allDocsBlock).getByText("billing.pm_view.md"));
    expect(onSelectDoc).toHaveBeenCalledWith("doc-folded-view");
  });

  it("shows non-current non-view docs in All Docs", async () => {
    const internalDoc = createMockDoc({
      title: "Internal README",
      docType: "design",
      folder: `${INTERNAL_WS_PREFIX}demo/README.md`,
    });
    const props = defaultSidebarProps({ docs: [internalDoc] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    await waitFor(() => {
      expect(screen.getByText("README.md")).toBeInTheDocument();
    });
    await userEvent.click(
      docFolderButton(screen.getByTestId("sidebar-all-docs-tab"), "README.md"),
    );

    expect(screen.getByText("Internal README")).toBeInTheDocument();
  });

  it("classifies non-view-type docs under All Docs", async () => {
    const nestedInternalDoc = createMockDoc({
      title: "Nested Internal",
      docType: "note",
      folder: `designs/${INTERNAL_WS_PREFIX}demo`,
      filePath: `designs/${INTERNAL_WS_PREFIX}demo/RUNBOOK.md`,
    });
    const props = defaultSidebarProps({ docs: [nestedInternalDoc] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    await waitFor(() => {
      expect(screen.getByText("RUNBOOK.md")).toBeInTheDocument();
    });

    const docsBlock = screen.getByTestId("sidebar-docs-tab");
    const allDocsBlock = screen.getByTestId("sidebar-all-docs-tab");

    expect(within(allDocsBlock).getByText("RUNBOOK.md")).toBeInTheDocument();
    expect(within(docsBlock).queryByText("RUNBOOK.md")).not.toBeInTheDocument();
  });

  it("renders docs/ contents without the literal docs root folder", async () => {
    const user = userEvent.setup();
    const nestedProjectDoc = createMockDoc({
      title: "Receiver CRUD Review",
      docType: "design",
      folder: "docs/experiments",
      filePath: "docs/experiments/receiver-crud-branch-fix-review.md",
      preview: "Experiment review",
    });
    const props = defaultSidebarProps({ docs: [nestedProjectDoc] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");
    const allDocsBlock = screen.getByTestId("sidebar-all-docs-tab");
    await waitFor(() => {
      expect(within(allDocsBlock).queryByText("docs")).not.toBeInTheDocument();
      expect(within(allDocsBlock).getByText("experiments")).toBeInTheDocument();
      expect(
        within(allDocsBlock).queryByText("receiver-crud-branch-fix-review.md"),
      ).not.toBeInTheDocument();
    });
    const experimentsFolder = docFolderButton(allDocsBlock, "experiments");
    expect(experimentsFolder).toHaveAttribute("aria-expanded", "false");

    expect(
      within(allDocsBlock).queryByText("docs/experiments"),
    ).not.toBeInTheDocument();

    await user.click(experimentsFolder);
    await waitFor(() => {
      expect(experimentsFolder).toHaveAttribute("aria-expanded", "true");
      expect(
        within(allDocsBlock).getByText("receiver-crud-branch-fix-review.md"),
      ).toBeInTheDocument();
    });
  });

  it("preserves All Docs folder toggles across doc list refetches", async () => {
    const user = userEvent.setup();
    const nestedProjectDoc = createMockDoc({
      title: "Receiver CRUD Review",
      docType: "design",
      folder: "docs/experiments",
      filePath: "docs/experiments/receiver-crud-branch-fix-review.md",
      preview: "Experiment review",
    });
    renderApp(<SidebarDocRefetchHarness docs={[nestedProjectDoc]} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");
    const allDocsBlock = screen.getByTestId("sidebar-all-docs-tab");
    await waitFor(() => {
      expect(within(allDocsBlock).getByText("experiments")).toBeInTheDocument();
    });

    const experimentsFolder = docFolderButton(allDocsBlock, "experiments");
    expect(experimentsFolder).toHaveAttribute("aria-expanded", "false");
    expect(
      within(allDocsBlock).queryByText("receiver-crud-branch-fix-review.md"),
    ).not.toBeInTheDocument();

    await user.click(experimentsFolder);
    await waitFor(() => {
      expect(
        within(allDocsBlock).getByText("receiver-crud-branch-fix-review.md"),
      ).toBeInTheDocument();
    });

    await user.click(experimentsFolder);
    await waitFor(() => {
      expect(experimentsFolder).toHaveAttribute("aria-expanded", "false");
      expect(
        within(allDocsBlock).queryByText("receiver-crud-branch-fix-review.md"),
      ).not.toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Refetch docs" }));

    const rerenderedAllDocsBlock = screen.getByTestId("sidebar-all-docs-tab");
    await waitFor(() => {
      expect(
        docFolderButton(rerenderedAllDocsBlock, "experiments"),
      ).toHaveAttribute("aria-expanded", "false");
    });

    const collapsedExperimentsFolder = docFolderButton(
      rerenderedAllDocsBlock,
      "experiments",
    );
    await user.click(collapsedExperimentsFolder);

    await waitFor(() => {
      expect(
        within(rerenderedAllDocsBlock).getByText(
          "receiver-crud-branch-fix-review.md",
        ),
      ).toBeInTheDocument();
    });
  });

  it("preserves All Docs folder state when the section is collapsed and reopened", async () => {
    const user = userEvent.setup();
    const archivedDoc = createMockDoc({
      title: "Archived Runbook",
      docType: "design",
      folder: "docs/archive",
      filePath: "docs/archive/runbook.md",
      preview: "Archived runbook",
    });
    renderApp(
      <LeftSidebar {...defaultSidebarProps({ docs: [archivedDoc] })} />,
      {
        route: "/org-g5labs/test-workspace",
      },
    );

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    const allDocsBlock = screen.getByTestId("sidebar-all-docs-tab");
    // Section header carries the display title "Archive" (left-sidebar D19).
    const allDocsToggle = within(allDocsBlock).getByRole("button", {
      name: "Archive",
    });
    const archiveFolder = await waitFor(() =>
      docFolderButton(allDocsBlock, "archive"),
    );
    expect(archiveFolder).toHaveAttribute("aria-expanded", "false");
    expect(
      within(allDocsBlock).queryByText("runbook.md"),
    ).not.toBeInTheDocument();

    await user.click(archiveFolder);
    await waitFor(() => {
      expect(archiveFolder).toHaveAttribute("aria-expanded", "true");
      expect(within(allDocsBlock).getByText("runbook.md")).toBeInTheDocument();
    });

    await user.click(allDocsToggle);
    await waitFor(() => {
      expect(allDocsToggle).toHaveAttribute("aria-expanded", "false");
      expect(
        within(allDocsBlock).queryByText("archive"),
      ).not.toBeInTheDocument();
    });
    await user.click(allDocsToggle);
    const reopenedArchiveFolder = await waitFor(() =>
      docFolderButton(allDocsBlock, "archive"),
    );
    expect(reopenedArchiveFolder).toHaveAttribute("aria-expanded", "true");
    expect(within(allDocsBlock).getByText("runbook.md")).toBeInTheDocument();

    await user.click(reopenedArchiveFolder);
    await waitFor(() => {
      expect(reopenedArchiveFolder).toHaveAttribute("aria-expanded", "false");
      expect(
        within(allDocsBlock).queryByText("runbook.md"),
      ).not.toBeInTheDocument();
    });

    await user.click(allDocsToggle);
    await waitFor(() =>
      expect(allDocsToggle).toHaveAttribute("aria-expanded", "false"),
    );
    await user.click(allDocsToggle);
    const collapsedArchiveFolder = await waitFor(() =>
      docFolderButton(allDocsBlock, "archive"),
    );
    expect(collapsedArchiveFolder).toHaveAttribute("aria-expanded", "false");
    expect(
      within(allDocsBlock).queryByText("runbook.md"),
    ).not.toBeInTheDocument();
  });

  it("strips internal workspace marker and legacy designs prefix from visible text", async () => {
    const internalDoc = createMockDoc({
      title: "Internal Security Plan",
      docType: "design",
      folder: `designs/${INTERNAL_WS_PREFIX}demo`,
      filePath: `designs/${INTERNAL_WS_PREFIX}demo/security.md`,
      preview: undefined,
    });
    const props = defaultSidebarProps({ docs: [internalDoc] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    await waitFor(() => {
      expect(screen.getByText("security.md")).toBeInTheDocument();
    });

    expect(
      screen.queryByText(new RegExp(INTERNAL_WS_PREFIX)),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("designs")).not.toBeInTheDocument();
    expect(screen.getByText("security.md")).toBeInTheDocument();
  });

  it("splits docs correctly between sections", async () => {
    const user = userEvent.setup();
    const explainerDoc = createMockDoc({
      title: "Explainer Alpha",
      name: "alpha-2026-08-05-0900.html",
      docType: "explainer",
      category: "explainer",
      filePath: `${WS_FOLDER}/explainers/alpha-2026-08-05-0900.html`,
    });
    const viewDoc = createMockDoc({
      title: "View Alpha",
      docType: "pm_view",
      folder: "Design",
    });
    const projectDoc = createMockDoc({
      title: "Project Beta",
      docType: "design",
      folder: `${INTERNAL_WS_PREFIX}proj/RUNBOOK.md`,
    });
    const props = defaultSidebarProps({
      docs: [explainerDoc, viewDoc, projectDoc],
      docsFolderPath: WS_FOLDER,
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");
    await waitFor(() => {
      expect(
        within(screen.getByTestId("sidebar-all-docs-tab")).getByText(
          "RUNBOOK.md",
        ),
      ).toBeInTheDocument();
      expect(
        within(screen.getByTestId("sidebar-docs-tab")).getByText("explainers"),
      ).toBeInTheDocument();
    });
    // The explainer is a Project Doc under its explainers/ subfolder.
    await user.click(
      docFolderButton(screen.getByTestId("sidebar-docs-tab"), "explainers"),
    );
    expect(
      within(screen.getByTestId("sidebar-docs-tab")).getByText(
        "alpha-2026-08-05-0900.html",
      ),
    ).toBeInTheDocument();
    await user.click(
      docFolderButton(screen.getByTestId("sidebar-all-docs-tab"), "RUNBOOK.md"),
    );
    expect(
      within(screen.getByTestId("sidebar-all-docs-tab")).getByText(
        "Project Beta",
      ),
    ).toBeInTheDocument();
    // View docs fold into All Docs (under their folder).
    await user.click(
      docFolderButton(screen.getByTestId("sidebar-all-docs-tab"), "Design"),
    );
    expect(
      within(screen.getByTestId("sidebar-all-docs-tab")).getByText(
        "View Alpha",
      ),
    ).toBeInTheDocument();
    const docsBlock = screen.getByTestId("sidebar-docs-tab");
    expect(
      within(docsBlock).queryByText("Project Beta"),
    ).not.toBeInTheDocument();
    expect(within(docsBlock).queryByText("View Alpha")).not.toBeInTheDocument();
  });

  it("does not show a Project Docs inventory badge", async () => {
    const internalDocs = [
      createMockDoc({
        title: "P1",
        docType: "design",
        folder: `${INTERNAL_WS_PREFIX}a`,
      }),
      createMockDoc({
        title: "P2",
        docType: "note",
        folder: `${INTERNAL_WS_PREFIX}b`,
      }),
    ];
    const props = defaultSidebarProps({ docs: internalDocs });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await waitFor(() => {
      expect(screen.getByText("Project Docs")).toBeInTheDocument();
    });

    const docsHeader = screen
      .getByTestId("sidebar-docs-tab")
      .querySelector('[role="button"]');
    expect(docsHeader?.textContent).not.toContain(String(internalDocs.length));
  });

  it("docs with view docType appear in All Docs regardless of folder", async () => {
    const noFolderDoc = createMockDoc({
      title: "No Folder",
      docType: "pm_view",
    });
    // Explicitly remove folder
    delete noFolderDoc.folder;
    const props = defaultSidebarProps({ docs: [noFolderDoc] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    await waitFor(() => {
      expect(
        within(screen.getByTestId("sidebar-all-docs-tab")).getByText(
          "No Folder",
        ),
      ).toBeInTheDocument();
    });
    expect(
      within(screen.getByTestId("sidebar-docs-tab")).queryByText("No Folder"),
    ).not.toBeInTheDocument();
  });

  it("shows the All Docs empty message when docs is empty", async () => {
    const props = defaultSidebarProps({ docs: [] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");
    await waitFor(() => {
      expect(screen.getByText("No documents found")).toBeInTheDocument();
    });
    expect(screen.queryByText("No explainers yet")).toBeNull();
  });

  it('shows "No documents found" in All Docs when only this workspace\'s explainer exists', async () => {
    const explainerDoc = createMockDoc({
      title: "Only Explainer",
      name: "only-2026-08-06-1000.html",
      docType: "explainer",
      category: "explainer",
      filePath: `${WS_FOLDER}/explainers/only-2026-08-06-1000.html`,
    });
    const props = defaultSidebarProps({
      docs: [explainerDoc],
      docsFolderPath: WS_FOLDER,
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    // All Docs is empty (the only doc is this workspace's explainer).
    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");
    await waitFor(() => {
      const msgs = screen.getAllByText("No documents found");
      expect(msgs.length).toBe(1);
    });

    // The explainer doc lives in Project Docs under explainers/.
    const docsBlock = screen.getByTestId("sidebar-docs-tab");
    await userEvent.click(docFolderButton(docsBlock, "explainers"));
    expect(
      within(docsBlock).getByText("only-2026-08-06-1000.html"),
    ).toBeInTheDocument();
  });

  it("treats view docs as plain All Docs entries when cloud workflows are disabled", async () => {
    const user = userEvent.setup();
    const viewDoc = createMockDoc({
      title: "Worker View Notes",
      docType: "pm_view",
      filePath: "views/worker-created.pm_view.md",
    });
    const props = defaultSidebarProps({
      docs: [viewDoc],
      capabilities: MINIMAL_WORKSPACE_UI_CAPABILITIES,
    });

    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    await waitFor(() => {
      expect(screen.getByText("views")).toBeInTheDocument();
    });
    await user.click(
      docFolderButton(screen.getByTestId("sidebar-all-docs-tab"), "views"),
    );

    expect(screen.queryByText("Explainers")).not.toBeInTheDocument();
    expect(screen.queryByText("Task Queue")).not.toBeInTheDocument();
    expect(screen.getByText("Project Docs")).toBeInTheDocument();

    const allDocsSection = screen.getByTestId("sidebar-all-docs-tab");
    expect(
      within(allDocsSection).getByText("worker-created.pm_view.md"),
    ).toBeInTheDocument();
  });
});

// =============================================================================
// Test Suite: Sidebar Section Ordering
// =============================================================================

describe("sidebar section ordering", () => {
  it("renders all first-class stacked section headers", async () => {
    const props = defaultSidebarProps();
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await waitFor(() => {
      expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
    });
    for (const id of [
      "sidebar-docs-tab",
      "sidebar-all-docs-tab",
      "sidebar-source-control-tab",
      "sidebar-tasks-tab",
      "sidebar-files-tab",
    ] as const) {
      expect(screen.getByTestId(id)).toBeInTheDocument();
    }
    expect(screen.queryByTestId("sidebar-explainers-tab")).toBeNull();
  });

  it("renders Project Docs before the All Docs section in the default order", async () => {
    const props = defaultSidebarProps();
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    await waitFor(() => {
      expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
      expect(screen.getByTestId("sidebar-all-docs-tab")).toBeInTheDocument();
    });

    const docsBlock = screen.getByTestId("sidebar-docs-tab");
    const allDocsBlock = screen.getByTestId("sidebar-all-docs-tab");
    // compareDocumentPosition bit 4 = DOCUMENT_POSITION_FOLLOWING
    const position = docsBlock.compareDocumentPosition(allDocsBlock);
    expect(position & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

// =============================================================================
// Test Suite: No per-row arrows
// =============================================================================

describe("per-row arrows removed", () => {
  it("doc items do NOT render uplift/downlift arrow buttons", async () => {
    const doc = createMockDoc({ title: "Arrow Test Doc" });
    const props = defaultSidebarProps({ docs: [doc] });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    await waitFor(() => {
      expect(screen.getByText("Arrow Test Doc")).toBeInTheDocument();
    });

    // There should be no buttons with title containing "uplift" or "downlift"
    const allButtons = document.querySelectorAll("button");
    for (const btn of allButtons) {
      const title = btn.getAttribute("title") || "";
      expect(title.toLowerCase()).not.toContain("uplift");
      expect(title.toLowerCase()).not.toContain("downlift");
    }
  });
});

// =============================================================================
// Test Suite: First-class Project Docs and All Docs sections
// =============================================================================
// Current workspace docs render only in Project Docs. All Docs keeps its label
// but hides the literal docs/ root and excludes current Project Docs.

describe("first-class docs sections", () => {
  const WS_ID = "11111111-1111-4111-8111-111111111111";
  const FOLDER = `docs/${WS_ID}`;

  it('Project Docs lists exactly the docs under docsFolderPath, under the static "Project Docs" header', async () => {
    const insideDoc = createMockDoc({
      title: "Inside Doc",
      name: "Inside Doc",
      docType: "design",
      filePath: `${FOLDER}/inside.md`,
    });
    const nestedInsideDoc = createMockDoc({
      title: "Nested Inside Doc",
      name: "Nested Inside Doc",
      docType: "design",
      filePath: `${FOLDER}/guides/nested.md`,
    });
    const outsideDoc = createMockDoc({
      title: "Outside Doc",
      name: "Outside Doc",
      docType: "design",
      filePath: "docs/outside.md",
      folder: "docs",
    });
    const props = defaultSidebarProps({
      docs: [insideDoc, nestedInsideDoc, outsideDoc],
      docsFolderPath: FOLDER,
      workspaceNamesById: { [WS_ID]: "Test Workspace" },
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");

    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    await waitFor(() => {
      expect(within(wsBlock).getByText("inside.md")).toBeInTheDocument();
    });
    expect(within(wsBlock).queryByText("outside.md")).not.toBeInTheDocument();
    expect(within(wsBlock).queryByText("nested.md")).not.toBeInTheDocument();

    const labels = docFolderLabels(wsBlock);
    expect(labels.some((label) => /^docs\b/.test(label))).toBe(false);
    expect(labels.some((label) => label.includes(WS_ID))).toBe(false);
    expect(labels.some((label) => label.includes("Test Workspace"))).toBe(
      false,
    );
    expect(labels.some((label) => label.includes("guides"))).toBe(true);

    const guidesFolder = docFolderButton(wsBlock, "guides");
    expect(guidesFolder).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(guidesFolder);
    await waitFor(() => {
      expect(guidesFolder).toHaveAttribute("aria-expanded", "true");
      expect(within(wsBlock).getByText("nested.md")).toBeInTheDocument();
    });

    // Section header is the static "Project Docs" label (D11).
    expect(within(wsBlock).getByText("Project Docs")).toBeInTheDocument();
    expect(
      within(wsBlock).queryByText("Test Workspace"),
    ).not.toBeInTheDocument();
  });

  it("marks Project Docs rows with their document source and shows no category row above them", async () => {
    const user = userEvent.setup();
    const onSelectDoc = vi.fn();
    const docs = [
      createMockDoc({
        name: "User Doc",
        filePath: `${FOLDER}/user.md`,
        documentSource: "user",
      }),
      createMockDoc({
        name: "Auxiliary Doc",
        filePath: `${FOLDER}/auxiliary.md`,
        documentSource: "auxiliary",
      }),
      createMockDoc({
        name: "Legacy Doc",
        filePath: `${FOLDER}/unlabeled.md`,
        documentSource: null,
      }),
      createMockDoc({
        name: "Rolling Deploy Doc",
        filePath: `${FOLDER}/missing-source.md`,
      }),
    ];
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          docs,
          docsFolderPath: FOLDER,
          onSelectDoc,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    activateSidebarSection("sidebar-docs-tab");
    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    // No All / User / Auxiliary / Unlabeled row (owner instruction,
    // 2026-10-30): every doc of the folder is listed, the row marker is the
    // only source cue.
    expect(
      within(wsBlock).queryByTestId("project-docs-source-filter"),
    ).not.toBeInTheDocument();
    expect(
      within(wsBlock).queryByRole("button", { name: "Auxiliary" }),
    ).not.toBeInTheDocument();
    for (const name of [
      "user.md",
      "auxiliary.md",
      "unlabeled.md",
      "missing-source.md",
    ]) {
      expect(within(wsBlock).getByText(name)).toBeInTheDocument();
    }
    const userSourceMarker = within(wsBlock).getByLabelText("Source: User");
    expect(userSourceMarker).toBeInTheDocument();
    expect(
      within(wsBlock).getByLabelText("Source: Auxiliary"),
    ).toBeInTheDocument();
    expect(within(wsBlock).getAllByLabelText(/^Source:/)).toHaveLength(2);
    const userDocRow = userSourceMarker.closest('[data-testid="doc-item"]');
    expect(userDocRow).not.toBeNull();
    const sourceTooltip = within(userDocRow as HTMLElement).getByRole(
      "tooltip",
    );
    expect(sourceTooltip).toHaveTextContent("Uploaded or created by a user");
    expect(userDocRow).toHaveAttribute("aria-describedby", sourceTooltip.id);
    (userDocRow as HTMLElement).focus();
    expect(userDocRow).toHaveFocus();
    expect(sourceTooltip).toHaveClass("group-focus-within/doc:opacity-100");
    await user.keyboard("{Enter}");
    expect(onSelectDoc).toHaveBeenCalledWith(docs[0].id);
    onSelectDoc.mockClear();
    await user.keyboard(" ");
    expect(onSelectDoc).toHaveBeenCalledWith(docs[0].id);
  });

  it("preserves Project Docs folder state across refetch and section collapse", async () => {
    const user = userEvent.setup();
    const nestedDoc = createMockDoc({
      title: "Nested State Doc",
      name: "Nested State Doc",
      docType: "design",
      filePath: `${FOLDER}/guides/state.md`,
    });
    renderApp(
      <SidebarDocRefetchHarness
        docs={[nestedDoc]}
        overrides={{ docsFolderPath: FOLDER }}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    activateSidebarSection("sidebar-docs-tab");
    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    const projectDocsToggle = within(wsBlock).getByRole("button", {
      name: "Project Docs",
    });
    const guidesFolder = await waitFor(() =>
      docFolderButton(wsBlock, "guides"),
    );
    expect(guidesFolder).toHaveAttribute("aria-expanded", "false");

    await user.click(guidesFolder);
    await waitFor(() => {
      expect(guidesFolder).toHaveAttribute("aria-expanded", "true");
      expect(within(wsBlock).getByText("state.md")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Refetch docs" }));
    await waitFor(() => {
      expect(docFolderButton(wsBlock, "guides")).toHaveAttribute(
        "aria-expanded",
        "true",
      );
      expect(within(wsBlock).getByText("state.md")).toBeInTheDocument();
    });

    await user.click(projectDocsToggle);
    await waitFor(() => {
      expect(projectDocsToggle).toHaveAttribute("aria-expanded", "false");
    });
    await user.click(projectDocsToggle);
    await waitFor(() => {
      expect(projectDocsToggle).toHaveAttribute("aria-expanded", "true");
      expect(docFolderButton(wsBlock, "guides")).toHaveAttribute(
        "aria-expanded",
        "true",
      );
      expect(within(wsBlock).getByText("state.md")).toBeInTheDocument();
    });
  });

  it("doc-row path subtitles show the workspace name in place of the docs/<id> UUID (D8)", async () => {
    const insideDoc = createMockDoc({
      title: "Inside Doc",
      name: "Inside Doc",
      docType: "design",
      filePath: `${FOLDER}/inside.md`,
      folder: FOLDER,
    });
    const props = defaultSidebarProps({
      docs: [insideDoc],
      docsFolderPath: FOLDER,
      workspaceNamesById: { [WS_ID]: "Test Workspace" },
    });
    renderApp(
      <WorkspaceNamesProvider value={{ [WS_ID]: "Test Workspace" }}>
        <LeftSidebar {...props} />
      </WorkspaceNamesProvider>,
      { route: "/org-g5labs/test-workspace" },
    );

    activateSidebarSection("sidebar-docs-tab");
    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    // The path subtitle relabels the docs/<uuid> prefix to the workspace name;
    // the stored filePath is unchanged (display-only).
    await waitFor(() => {
      expect(
        within(wsBlock).getByText("docs/Test Workspace/inside.md"),
      ).toBeInTheDocument();
    });
    expect(
      within(wsBlock).queryByText(`${FOLDER}/inside.md`),
    ).not.toBeInTheDocument();
  });

  it("doc-row path subtitles fall back to the raw UUID without a workspace-name context", async () => {
    const insideDoc = createMockDoc({
      title: "Inside Doc",
      name: "Inside Doc",
      docType: "design",
      filePath: `${FOLDER}/inside.md`,
      folder: FOLDER,
    });
    const props = defaultSidebarProps({
      docs: [insideDoc],
      docsFolderPath: FOLDER,
      workspaceNamesById: { [WS_ID]: "Test Workspace" },
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    await waitFor(() => {
      expect(
        within(wsBlock).getByText(`${FOLDER}/inside.md`),
      ).toBeInTheDocument();
    });
  });

  it("Project Docs shows an empty state while All Docs hides the docs root and current empty folder", async () => {
    const outsideDoc = createMockDoc({
      title: "Outside Doc",
      name: "Outside Doc",
      docType: "design",
      filePath: "docs/archive/outside.md",
      folder: "docs/archive",
    });
    const props = defaultSidebarProps({
      docs: [outsideDoc],
      docsFolderPath: FOLDER,
      workspaceNamesById: { [WS_ID]: "Test Workspace" },
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    expect(
      within(wsBlock).getByText("No project docs yet"),
    ).toBeInTheDocument();

    const allBlock = screen.getByTestId("sidebar-all-docs-tab");
    expect(within(allBlock).getByText("archive")).toBeInTheDocument();
    expect(within(allBlock).queryByText("outside.md")).not.toBeInTheDocument();
    await userEvent.click(docFolderButton(allBlock, "archive"));
    expect(within(allBlock).getByText("outside.md")).toBeInTheDocument();
    expect(within(allBlock).queryByText("docs")).not.toBeInTheDocument();
    expect(
      within(allBlock).queryByText("Test Workspace"),
    ).not.toBeInTheDocument();
  });

  it("uses one root highlight across the Project Docs header and empty body", async () => {
    const onDropProjectDocFiles = vi.fn();
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          docsFolderPath: FOLDER,
          onDropProjectDocFiles,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    activateSidebarSection("sidebar-docs-tab");

    const docsBlock = await screen.findByTestId("sidebar-docs-tab");
    const header = within(docsBlock).getByRole("button", {
      name: "Project Docs",
    });
    const dropZone = within(docsBlock).getByTestId("project-docs-drop-zone");
    const headerStatus = within(docsBlock).getByTestId(
      "sidebar-section-header-drop-status",
    );
    const files = [
      new File(["# Root"], "root.md", { type: "text/markdown" }),
      new File(["<html>Root</html>"], "root.html", {
        type: "text/html",
      }),
    ];

    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer(files),
    });
    expect(docsBlock).toHaveClass("ring-[var(--g5-active)]");
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(dropZone).not.toHaveClass("ring-[var(--g5-active)]");
    expect(headerStatus).toHaveTextContent(`Drop files into ${FOLDER}`);

    fireEvent.dragLeave(header, {
      dataTransfer: fileDataTransfer(files),
      relatedTarget: dropZone,
    });
    expect(docsBlock).not.toHaveClass("ring-[var(--g5-active)]");
    expect(headerStatus).toHaveTextContent("");

    fireEvent.dragEnter(dropZone, {
      dataTransfer: fileDataTransfer(files),
    });

    expect(within(dropZone).getByRole("status")).toHaveTextContent(
      `Drop files into ${FOLDER}`,
    );
    expect(docsBlock).toHaveClass("ring-[var(--g5-active)]");
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(dropZone).not.toHaveClass("ring-[var(--g5-active)]");

    fireEvent.drop(dropZone, { dataTransfer: fileDataTransfer(files) });
    expect(onDropProjectDocFiles).toHaveBeenCalledTimes(1);
    expect(onDropProjectDocFiles).toHaveBeenCalledWith({
      files,
      folderPath: FOLDER,
    });
    expect(within(dropZone).getByRole("status")).toHaveTextContent("");
    expect(docsBlock).not.toHaveClass("ring-[var(--g5-active)]");
  });

  it("accepts populated root drops on the Project Docs header while expanded or collapsed", async () => {
    const onDropProjectDocFiles = vi.fn();
    const directDoc = createMockDoc({
      id: "populated-header-drop",
      name: "existing.md",
      filePath: `${FOLDER}/existing.md`,
      folder: FOLDER,
    });
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          docs: [directDoc],
          docsFolderPath: FOLDER,
          onDropProjectDocFiles,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    activateSidebarSection("sidebar-docs-tab");

    const docsBlock = await screen.findByTestId("sidebar-docs-tab");
    const header = within(docsBlock).getByRole("button", {
      name: "Project Docs",
    });
    const status = within(docsBlock).getByTestId(
      "sidebar-section-header-drop-status",
    );
    const firstFiles = [
      new File(["# Root"], "root.md", { type: "text/markdown" }),
      new File(["<html>Root</html>"], "root.html", {
        type: "text/html",
      }),
    ];

    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer([], ["text/plain"]),
    });
    fireEvent.drop(header, {
      dataTransfer: fileDataTransfer([], ["text/plain"]),
    });
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(status).toHaveTextContent("");
    expect(onDropProjectDocFiles).not.toHaveBeenCalled();

    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer([]),
    });
    fireEvent.drop(header, {
      dataTransfer: fileDataTransfer([]),
    });
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(status).toHaveTextContent("");
    expect(onDropProjectDocFiles).not.toHaveBeenCalled();

    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer(firstFiles),
    });
    expect(header).toHaveClass("ring-[var(--g5-active)]");
    expect(docsBlock).not.toHaveClass("ring-[var(--g5-active)]");
    expect(status).toHaveTextContent(`Drop files into ${FOLDER}`);

    fireEvent.dragLeave(header, {
      dataTransfer: fileDataTransfer(firstFiles),
      relatedTarget: document.body,
    });
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(status).toHaveTextContent("");

    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer(firstFiles),
    });
    fireEvent.drop(header, {
      dataTransfer: fileDataTransfer(firstFiles),
    });
    expect(onDropProjectDocFiles).toHaveBeenCalledTimes(1);
    expect(onDropProjectDocFiles).toHaveBeenLastCalledWith({
      files: firstFiles,
      folderPath: FOLDER,
    });
    expect(status).toHaveTextContent("");

    await userEvent.click(header);
    expect(header).toHaveAttribute("aria-expanded", "false");
    const collapsedFiles = [
      new File(["# Collapsed"], "collapsed.md", { type: "text/markdown" }),
    ];
    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer(collapsedFiles),
    });
    fireEvent.drop(header, {
      dataTransfer: fileDataTransfer(collapsedFiles),
    });
    expect(onDropProjectDocFiles).toHaveBeenCalledTimes(2);
    expect(onDropProjectDocFiles).toHaveBeenLastCalledWith({
      files: collapsedFiles,
      folderPath: FOLDER,
    });
  });

  it("uses one exact nested-folder target without falling through to root or document rows", async () => {
    const onDropProjectDocFiles = vi.fn();
    const directDoc = createMockDoc({
      id: "direct-drop-exclusion",
      name: "direct.md",
      filePath: `${FOLDER}/direct.md`,
      folder: FOLDER,
    });
    const nestedDoc = createMockDoc({
      id: "nested-drop-target",
      name: "nested.md",
      filePath: `${FOLDER}/guides/nested.md`,
      folder: `${FOLDER}/guides`,
    });
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          docs: [directDoc, nestedDoc],
          docsFolderPath: FOLDER,
          onDropProjectDocFiles,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    activateSidebarSection("sidebar-docs-tab");

    const docsBlock = await screen.findByTestId("sidebar-docs-tab");
    const dropZone = within(docsBlock).getByTestId("project-docs-drop-zone");
    const header = within(docsBlock).getByRole("button", {
      name: "Project Docs",
    });
    const headerStatus = within(docsBlock).getByTestId(
      "sidebar-section-header-drop-status",
    );
    const guidesFolder = docFolderButton(docsBlock, "guides");
    const files = [
      new File(["# Nested"], "nested-drop.md", {
        type: "text/markdown",
      }),
    ];

    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer(files),
    });
    expect(header).toHaveClass("ring-[var(--g5-active)]");
    fireEvent.dragLeave(header, {
      dataTransfer: fileDataTransfer(files),
      relatedTarget: dropZone,
    });
    fireEvent.dragEnter(dropZone, {
      dataTransfer: fileDataTransfer(files),
    });
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(docsBlock).not.toHaveClass("ring-[var(--g5-active)]");
    expect(headerStatus).toHaveTextContent("");
    fireEvent.dragEnter(guidesFolder, {
      dataTransfer: fileDataTransfer(files),
    });
    expect(within(dropZone).getByRole("status")).toHaveTextContent(
      `Drop files into ${FOLDER}/guides`,
    );
    expect(guidesFolder).toHaveClass("ring-[var(--g5-active)]");
    expect(dropZone).not.toHaveClass("ring-[var(--g5-active)]");

    fireEvent.drop(guidesFolder, {
      dataTransfer: fileDataTransfer(files),
    });
    expect(onDropProjectDocFiles).toHaveBeenCalledTimes(1);
    expect(onDropProjectDocFiles).toHaveBeenLastCalledWith({
      files,
      folderPath: `${FOLDER}/guides`,
    });

    const directRow = within(docsBlock)
      .getByText("direct.md")
      .closest('[data-testid="doc-item"]');
    expect(directRow).not.toBeNull();
    fireEvent.drop(directRow!, {
      dataTransfer: fileDataTransfer(files),
    });
    fireEvent.drop(dropZone, {
      dataTransfer: fileDataTransfer([], ["text/plain"]),
    });
    expect(onDropProjectDocFiles).toHaveBeenCalledTimes(1);
  });

  it("does not activate Project Docs or All Docs without a writable drop callback", async () => {
    const outsideDoc = createMockDoc({
      name: "outside.md",
      filePath: "docs/archive/outside.md",
      folder: "docs/archive",
    });
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          docs: [outsideDoc],
          docsFolderPath: FOLDER,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    const docsBlock = await screen.findByTestId("sidebar-docs-tab");
    const dropZone = within(docsBlock).getByTestId("project-docs-drop-zone");
    const header = within(docsBlock).getByRole("button", {
      name: "Project Docs",
    });
    fireEvent.dragEnter(header, {
      dataTransfer: fileDataTransfer([
        new File(["# Read only header"], "readonly-header.md", {
          type: "text/markdown",
        }),
      ]),
    });
    expect(header).not.toHaveClass("ring-[var(--g5-active)]");
    expect(
      within(docsBlock).queryByTestId("sidebar-section-header-drop-status"),
    ).not.toBeInTheDocument();

    fireEvent.dragEnter(dropZone, {
      dataTransfer: fileDataTransfer([
        new File(["# Read only"], "readonly.md", {
          type: "text/markdown",
        }),
      ]),
    });
    expect(within(dropZone).getByRole("status")).toHaveTextContent("");
    expect(dropZone).not.toHaveClass("ring-[var(--g5-active)]");
    expect(docsBlock).not.toHaveClass("ring-[var(--g5-active)]");

    expect(
      within(screen.getByTestId("sidebar-all-docs-tab")).queryByTestId(
        "project-docs-drop-zone",
      ),
    ).not.toBeInTheDocument();
  });

  it("All Docs and Project Docs open documents by identity", async () => {
    const user = userEvent.setup();
    const onSelectDoc = vi.fn();
    const insideDoc = createMockDoc({
      id: "doc-inside-ro",
      title: "Inside Doc",
      name: "Inside Doc",
      docType: "design",
      filePath: `${FOLDER}/inside.md`,
      folder: FOLDER,
    });
    const archivedDoc = createMockDoc({
      id: "doc-archive-ro",
      title: "Archived Doc",
      name: "Archived Doc",
      docType: "design",
      filePath: "docs/archive/archived.md",
      folder: "docs/archive",
    });
    const props = defaultSidebarProps({
      docs: [insideDoc, archivedDoc],
      docsFolderPath: FOLDER,
      workspaceNamesById: { [WS_ID]: "Test Workspace" },
      onSelectDoc,
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    await user.click(within(wsBlock).getByText("inside.md"));
    expect(onSelectDoc).toHaveBeenLastCalledWith("doc-inside-ro");

    const allBlock = screen.getByTestId("sidebar-all-docs-tab");
    expect(within(allBlock).queryByText("inside.md")).not.toBeInTheDocument();
    await user.click(docFolderButton(allBlock, "archive"));
    await user.click(within(allBlock).getByText("archived.md"));
    expect(onSelectDoc).toHaveBeenLastCalledWith("doc-archive-ro");
  });

  it("All Docs keeps non-current dirty docs from the active workspace state", async () => {
    const dirtyMergedDoc = createMockDoc({
      id: "doc-dirty-merged",
      title: "Dirty Merged Doc",
      name: "Dirty Merged Doc",
      docType: "design",
      filePath: "docs/archive/dirty-merged.md",
      folder: "docs/archive",
      source: "merged",
      draftDirty: true,
    });
    const dirtyOverlayDoc = createMockDoc({
      id: "overlay:repo:ZG9jcy9hcmNoaXZlL2RpcnR5LW92ZXJsYXkubWQ",
      title: "Dirty Overlay Doc",
      name: "Dirty Overlay Doc",
      docType: "design",
      filePath: "docs/archive/dirty-overlay.md",
      folder: "docs/archive",
      source: "overlay",
      draftDirty: true,
      isVirtual: true,
    });
    const currentWorkspaceDirtyDoc = createMockDoc({
      id: "doc-current-dirty",
      title: "Current Dirty Doc",
      name: "Current Dirty Doc",
      docType: "design",
      filePath: `${FOLDER}/current-dirty.md`,
      folder: FOLDER,
      source: "merged",
      draftDirty: true,
    });
    const props = defaultSidebarProps({
      docs: [dirtyMergedDoc, dirtyOverlayDoc, currentWorkspaceDirtyDoc],
      docsFolderPath: FOLDER,
      workspaceNamesById: { [WS_ID]: "Test Workspace" },
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-all-docs-tab");

    const allBlock = await screen.findByTestId("sidebar-all-docs-tab");
    await userEvent.click(docFolderButton(allBlock, "archive"));

    expect(within(allBlock).getByText("dirty-merged.md")).toBeInTheDocument();
    expect(within(allBlock).getByText("dirty-overlay.md")).toBeInTheDocument();
    expect(
      within(allBlock).queryByText("current-dirty.md"),
    ).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------
  // Explainers are Project Docs of the workspace that owns them
  // ---------------------------------------------------------------------------
  // The docs inventory spans the whole docs branch to feed All Docs, so the
  // folder decides where an explainer is listed: this workspace's own under
  // Project Docs `explainers/`, every other workspace's and the legacy root
  // under All Docs (owner instruction, 2026-10-30: no Explainers section).

  const OTHER_WS_ID = "22222222-2222-4222-8222-222222222222";
  const OTHER_FOLDER = `docs/${OTHER_WS_ID}`;

  function explainerDoc(overrides: Partial<DocNode>): DocNode {
    return createMockDoc({
      docType: "explainer",
      category: "explainer",
      ...overrides,
    });
  }

  const ownExplainer = () =>
    explainerDoc({
      id: "doc-own-explainer",
      title: "Own Explainer",
      name: "own-2026-08-11-0900.html",
      filePath: `${FOLDER}/explainers/own-2026-08-11-0900.html`,
      folder: `${FOLDER}/explainers`,
    });
  const foreignExplainer = () =>
    explainerDoc({
      id: "doc-foreign-explainer",
      title: "Foreign Explainer",
      name: "foreign-2026-08-10-0900.html",
      filePath: `${OTHER_FOLDER}/explainers/foreign-2026-08-10-0900.html`,
      folder: `${OTHER_FOLDER}/explainers`,
    });
  const legacyExplainer = () =>
    explainerDoc({
      id: "doc-legacy-explainer",
      title: "Legacy Explainer",
      name: "legacy-2026-08-01-0900.html",
      filePath: "explainers/legacy-2026-08-01-0900.html",
      folder: "explainers",
    });

  it("Project Docs lists this workspace's own explainer under explainers/ and not foreign or legacy-root ones", async () => {
    const props = defaultSidebarProps({
      docs: [ownExplainer(), foreignExplainer(), legacyExplainer()],
      docsFolderPath: FOLDER,
      workspaceNamesById: {
        [WS_ID]: "Test Workspace",
        [OTHER_WS_ID]: "Other Workspace",
      },
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");

    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    await expandAllDocFolders(wsBlock);
    await waitFor(() => {
      expect(
        within(wsBlock).getByText("own-2026-08-11-0900.html"),
      ).toBeInTheDocument();
    });
    expect(within(wsBlock).getByText("explainers")).toBeInTheDocument();
    expect(
      within(wsBlock).queryByText("foreign-2026-08-10-0900.html"),
    ).not.toBeInTheDocument();
    expect(
      within(wsBlock).queryByText("legacy-2026-08-01-0900.html"),
    ).not.toBeInTheDocument();
  });

  it("keeps foreign and legacy-root explainers reachable through All Docs", async () => {
    const props = defaultSidebarProps({
      docs: [ownExplainer(), foreignExplainer(), legacyExplainer()],
      docsFolderPath: FOLDER,
      workspaceNamesById: {
        [WS_ID]: "Test Workspace",
        [OTHER_WS_ID]: "Other Workspace",
      },
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-all-docs-tab");

    const allBlock = await screen.findByTestId("sidebar-all-docs-tab");
    await expandAllDocFolders(allBlock);

    // Not this workspace's: reachable through All Docs.
    expect(
      within(allBlock).getByText("foreign-2026-08-10-0900.html"),
    ).toBeInTheDocument();
    expect(
      within(allBlock).getByText("legacy-2026-08-01-0900.html"),
    ).toBeInTheDocument();
    // This workspace's own explainer belongs to Project Docs, so All Docs
    // (which hides the current workspace folder) must not double-list it (D19).
    expect(
      within(allBlock).queryByText("own-2026-08-11-0900.html"),
    ).not.toBeInTheDocument();
  });

  it("re-scopes Project Docs' explainers when the workspace switches", async () => {
    function WorkspaceSwitchHarness({ docs }: { docs: DocNode[] }) {
      const [folder, setFolder] = useState(FOLDER);
      return (
        <>
          <button type="button" onClick={() => setFolder(OTHER_FOLDER)}>
            switch workspace
          </button>
          <LeftSidebar
            {...defaultSidebarProps({ docs, docsFolderPath: folder })}
          />
        </>
      );
    }

    renderApp(
      <WorkspaceSwitchHarness docs={[ownExplainer(), foreignExplainer()]} />,
      { route: "/org-g5labs/test-workspace" },
    );

    activateSidebarSection("sidebar-docs-tab");

    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    await expandAllDocFolders(wsBlock);
    await waitFor(() => {
      expect(
        within(wsBlock).getByText("own-2026-08-11-0900.html"),
      ).toBeInTheDocument();
    });
    expect(
      within(wsBlock).queryByText("foreign-2026-08-10-0900.html"),
    ).not.toBeInTheDocument();

    // Switching workspaces flips which explainer is "own" and which is foreign.
    await userEvent.click(screen.getByText("switch workspace"));

    const switched = screen.getByTestId("sidebar-docs-tab");
    await expandAllDocFolders(switched);
    await waitFor(() => {
      expect(
        within(switched).getByText("foreign-2026-08-10-0900.html"),
      ).toBeInTheDocument();
    });
    expect(
      within(switched).queryByText("own-2026-08-11-0900.html"),
    ).not.toBeInTheDocument();
  });

  it("lists no Project Docs while docsFolderPath is still unknown", async () => {
    const props = defaultSidebarProps({
      docs: [ownExplainer(), foreignExplainer()],
      docsFolderPath: null,
    });
    renderApp(<LeftSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });

    activateSidebarSection("sidebar-docs-tab");
    activateSidebarSection("sidebar-all-docs-tab");

    const wsBlock = await screen.findByTestId("sidebar-docs-tab");
    expect(within(wsBlock).queryAllByTestId("doc-item")).toHaveLength(0);

    // Nothing is lost: without a known workspace folder every doc stays in the
    // pool, and All Docs lists the whole inventory.
    const allBlock = screen.getByTestId("sidebar-all-docs-tab");
    await expandAllDocFolders(allBlock);
    expect(
      within(allBlock).getByText("own-2026-08-11-0900.html"),
    ).toBeInTheDocument();
    expect(
      within(allBlock).getByText("foreign-2026-08-10-0900.html"),
    ).toBeInTheDocument();
  });
});
