// =============================================================================
// Sidebar Header Button Tests
// =============================================================================
// Tests the PROJECT DOCS section header buttons:
// - PROJECT DOCS section has a direct create button and a context menu.
// - There is no Explainers section: explainers are Project Docs under
//   `explainers/` (owner instruction, 2026-10-30).
// - ISSUES retains its contextual actions menu.
// =============================================================================

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../test/render";
import { activateSidebarSection } from "../test/sidebar";

import { LeftSidebar } from "./tree/TreeNode";
import type { DocNode } from "../lib/types";

function defaultSidebarProps(overrides: Record<string, unknown> = {}) {
  return {
    isOpen: true,
    width: 280,
    workspaceId: "test-workspace",
    organization: "g5labs",
    branches: [],
    tasks: [],
    docs: [],
    selectedDocId: null as string | null,
    selectedTaskId: null as string | null,
    onSelectDoc: vi.fn(),
    onSelectFile: vi.fn(),
    onSelectTask: vi.fn(),
    searchQuery: "",
    ...overrides,
  };
}

function projectDoc(overrides: Partial<DocNode> = {}): DocNode {
  return {
    id: "doc-project-1",
    name: "Project Plan",
    docType: "design",
    filePath: "docs/project-plan.md",
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2026-01-01T00:00:00Z",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    title: "Project Plan",
    category: "design",
    preview: "Project notes",
    ...overrides,
  };
}

// =============================================================================
// Tests
// =============================================================================

describe("Sidebar header buttons", () => {
  // -------------------------------------------------------------------------
  // PROJECT DOCS section
  // -------------------------------------------------------------------------

  describe("PROJECT DOCS section", () => {
    it("renders an always-present direct create button when onCreateProjectDoc is provided", async () => {
      const onCreateProjectDoc = vi.fn();
      const props = defaultSidebarProps({ onCreateProjectDoc });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");

      await waitFor(() => {
        const docsTab = screen.getByTestId("sidebar-docs-tab");
        const createButton = within(docsTab).getByRole("button", {
          name: "Create project doc",
        });
        expect(createButton).toBeInTheDocument();
        expect(createButton).toHaveAttribute("title", "Create project doc");
        expect(
          within(docsTab).queryByLabelText("Project Docs actions"),
        ).not.toBeInTheDocument();
      });
    });

    it("calls onCreateProjectDoc from the direct create button", async () => {
      const user = userEvent.setup();
      const onCreateProjectDoc = vi.fn();
      const props = defaultSidebarProps({ onCreateProjectDoc });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");

      await waitFor(() => {
        expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
      });

      const docsTab = screen.getByTestId("sidebar-docs-tab");
      await user.click(within(docsTab).getByLabelText("Create project doc"));

      expect(onCreateProjectDoc).toHaveBeenCalledTimes(1);
    });

    it("does NOT render the direct create button when onCreateProjectDoc is NOT provided", async () => {
      const props = defaultSidebarProps(); // no onCreateProjectDoc
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");

      await waitFor(() => {
        expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
      });

      const docsTab = screen.getByTestId("sidebar-docs-tab");
      expect(
        within(docsTab).queryByLabelText("Create project doc"),
      ).not.toBeInTheDocument();
    });

    it("keeps the direct create button available without changing collapsed state", async () => {
      const user = userEvent.setup();
      const onCreateProjectDoc = vi.fn();
      const props = defaultSidebarProps({ onCreateProjectDoc, docs: [] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");

      await waitFor(() => {
        expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
      });

      const docsTab = screen.getByTestId("sidebar-docs-tab");

      // After expansion, the Project Docs header reports aria-expanded="true".
      const sectionButton = within(docsTab).getByRole("button", {
        name: "Project Docs",
      });
      await waitFor(() =>
        expect(sectionButton.getAttribute("aria-expanded")).toBe("true"),
      );

      await user.click(sectionButton);
      expect(sectionButton).toHaveAttribute("aria-expanded", "false");

      await user.click(within(docsTab).getByLabelText("Create project doc"));

      expect(onCreateProjectDoc).toHaveBeenCalledTimes(1);
      expect(sectionButton).toHaveAttribute("aria-expanded", "false");
    });

    it("opens the Project Docs context menu on right-click without an ellipsis or collapse change", async () => {
      const user = userEvent.setup();
      const onCreateProjectDoc = vi.fn();
      const props = defaultSidebarProps({ onCreateProjectDoc });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");
      const docsTab = await screen.findByTestId("sidebar-docs-tab");
      const sectionButton = within(docsTab).getByRole("button", {
        name: "Project Docs",
      });

      expect(sectionButton).toHaveAttribute("aria-expanded", "true");
      expect(
        within(docsTab).queryByRole("button", {
          name: "Project Docs actions",
        }),
      ).not.toBeInTheDocument();

      expect(fireEvent.contextMenu(sectionButton)).toBe(false);
      const menu = screen.getByRole("menu", {
        name: "Project Docs actions",
      });
      expect(
        within(menu).getByRole("menuitem", { name: "Create project doc" }),
      ).toBeInTheDocument();
      expect(sectionButton).toHaveAttribute("aria-expanded", "true");

      await user.click(
        within(menu).getByRole("menuitem", { name: "Create project doc" }),
      );

      expect(onCreateProjectDoc).toHaveBeenCalledTimes(1);
      expect(sectionButton).toHaveAttribute("aria-expanded", "true");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    });

    it("opens the Project Docs context menu from both keyboard context gestures", async () => {
      const user = userEvent.setup();
      const onCreateProjectDoc = vi.fn();
      const props = defaultSidebarProps({ onCreateProjectDoc });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");
      const docsTab = await screen.findByTestId("sidebar-docs-tab");
      const sectionButton = within(docsTab).getByRole("button", {
        name: "Project Docs",
      });

      expect(
        fireEvent.keyDown(sectionButton, { key: "F10", shiftKey: true }),
      ).toBe(false);
      let menu = screen.getByRole("menu", {
        name: "Project Docs actions",
      });
      expect(sectionButton).toHaveAttribute("aria-expanded", "true");
      await user.click(
        within(menu).getByRole("menuitem", { name: "Create project doc" }),
      );

      expect(onCreateProjectDoc).toHaveBeenCalledTimes(1);
      expect(sectionButton).toHaveAttribute("aria-expanded", "true");

      expect(fireEvent.keyDown(sectionButton, { key: "ContextMenu" })).toBe(
        false,
      );
      menu = screen.getByRole("menu", { name: "Project Docs actions" });
      expect(
        within(menu).getByRole("menuitem", { name: "Create project doc" }),
      ).toBeInTheDocument();
      expect(sectionButton).toHaveAttribute("aria-expanded", "true");
    });

    it("does not intercept Project Docs context gestures without a create callback", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");
      const docsTab = await screen.findByTestId("sidebar-docs-tab");
      const sectionButton = within(docsTab).getByRole("button", {
        name: "Project Docs",
      });

      expect(fireEvent.contextMenu(sectionButton)).toBe(true);
      expect(
        fireEvent.keyDown(sectionButton, { key: "F10", shiftKey: true }),
      ).toBe(true);
      expect(fireEvent.keyDown(sectionButton, { key: "ContextMenu" })).toBe(
        true,
      );

      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    });

    it.each([
      { label: "committed", draftDirty: false },
      { label: "draft-dirty", draftDirty: true },
    ])(
      "delegates Delete for production-shaped $label Project Doc rows",
      async ({ draftDirty }) => {
        const onDeleteProjectDoc = vi.fn();
        const repositoryHandle =
          "overlay:11111111-1111-4111-8111-111111111111:ZG9jcy90ZXN0LXdvcmtzcGFjZS9wcm9qZWN0LXBsYW4ubWQ";
        const props = defaultSidebarProps({
          docsFolderPath: "docs/test-workspace",
          docs: [
            projectDoc({
              id: repositoryHandle,
              filePath: "docs/test-workspace/project-plan.md",
              isVirtual: true,
              draftDirty,
            }),
          ],
          onDeleteProjectDoc,
        });
        renderApp(<LeftSidebar {...props} />, {
          route: "/org-g5labs/test-workspace",
        });

        activateSidebarSection("sidebar-docs-tab");

        await waitFor(() => {
          expect(
            within(screen.getByTestId("sidebar-docs-tab")).getByText(
              "project-plan.md",
            ),
          ).toBeInTheDocument();
        });

        expect(
          fireEvent.contextMenu(
            within(screen.getByTestId("sidebar-docs-tab")).getByText(
              "project-plan.md",
            ),
          ),
        ).toBe(false);
        expect(
          screen.getByRole("menu", { name: "Project Doc actions" }),
        ).toBeInTheDocument();
        await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));

        expect(onDeleteProjectDoc).toHaveBeenCalledWith(
          expect.objectContaining({ id: repositoryHandle, isVirtual: true }),
        );
      },
    );

    it("does not expose Project Doc deletion in All Docs read-only rows", async () => {
      const onDeleteProjectDoc = vi.fn();
      const props = defaultSidebarProps({
        docsFolderPath: "docs/test-workspace",
        docs: [projectDoc({ filePath: "docs/archive/project-plan.md" })],
        onDeleteProjectDoc,
      });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      const allDocsSection = await screen.findByTestId("sidebar-all-docs-tab");
      await waitFor(() => {
        expect(within(allDocsSection).getByText("archive")).toBeInTheDocument();
      });

      const archiveFolder = within(allDocsSection)
        .getByText("archive")
        .closest("button");
      if (!archiveFolder) throw new Error("archive folder not found");
      await userEvent.click(archiveFolder);

      fireEvent.contextMenu(
        within(allDocsSection).getByText("project-plan.md"),
      );

      expect(
        screen.queryByRole("menuitem", { name: "Delete" }),
      ).not.toBeInTheDocument();
      expect(onDeleteProjectDoc).not.toHaveBeenCalled();
    });

    it("does not expose Project Doc deletion for view rows folded into All Docs", async () => {
      const onDeleteProjectDoc = vi.fn();
      const props = defaultSidebarProps({
        docs: [
          projectDoc({
            id: "doc-view-1",
            name: "Receiver View",
            title: "Receiver View",
            docType: "pm_view",
            category: "pm_view",
            filePath: "views/receiver.pm_view.md",
          }),
        ],
        onDeleteProjectDoc,
      });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // View docs fold into the read-only All Docs listing (left-sidebar D36).
      activateSidebarSection("sidebar-all-docs-tab");
      const allDocsTab = screen.getByTestId("sidebar-all-docs-tab");
      await waitFor(() => {
        expect(within(allDocsTab).getByText("views")).toBeInTheDocument();
      });
      fireEvent.click(within(allDocsTab).getByText("views"));
      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("receiver.pm_view.md"),
        ).toBeInTheDocument();
      });

      fireEvent.contextMenu(
        within(allDocsTab).getByText("receiver.pm_view.md"),
      );

      expect(
        screen.queryByRole("menuitem", { name: "Delete" }),
      ).not.toBeInTheDocument();
    });
  });
});
