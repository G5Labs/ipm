// =============================================================================
// View Sidebar Rows — folded into All Docs
// =============================================================================
// The dedicated Views section was replaced by Explainers (left-sidebar D35).
// View docs fold into the read-only All Docs listing (left-sidebar D36) as
// ordinary rows under their views/** folders, with no inline sync/implement
// hover affordances. Sync/bootstrap lives in the main doc workflow.
// =============================================================================

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { activateSidebarSection } from "../../test/sidebar";

import { LeftSidebar } from "../tree/TreeNode";
import type { DocNode } from "../../lib/types";

// ---------------------------------------------------------------------------
// Test data — view docs
// ---------------------------------------------------------------------------

/** A test_view document */
const testViewDoc: DocNode = {
  id: "doc-test-view-1",
  name: "auth-module.test_view.md",
  docType: "test_view",
  filePath: "views/test_views/auth-module.test_view.md",
  linkedNodeCount: 3,
  lastUpliftedAt: null,
  lastDownliftedAt: null,
  contentUpdatedAt: "2026-02-25T10:00:00Z",
  createdAt: "2026-02-25T10:00:00Z",
  updatedAt: "2026-02-25T10:00:00Z",
  isFuture: false,
  title: "Auth Module Test View",
  category: "test_view",
};

/** A PM view document */
const pmViewDoc: DocNode = {
  id: "doc-pm-view-1",
  name: "billing.pm_view.md",
  docType: "pm_view",
  filePath: "views/pm_views/billing.pm_view.md",
  linkedNodeCount: 5,
  lastUpliftedAt: null,
  lastDownliftedAt: null,
  contentUpdatedAt: "2026-02-24T10:00:00Z",
  createdAt: "2026-02-24T10:00:00Z",
  updatedAt: "2026-02-24T10:00:00Z",
  isFuture: false,
  title: "Billing PM View",
  category: "pm_view",
};

/** An architect view document */
const archViewDoc: DocNode = {
  id: "doc-arch-view-1",
  name: "api.arch_view.md",
  docType: "arch_view",
  filePath: "views/arch_views/api.arch_view.md",
  linkedNodeCount: 2,
  lastUpliftedAt: null,
  lastDownliftedAt: null,
  contentUpdatedAt: "2026-02-23T10:00:00Z",
  createdAt: "2026-02-23T10:00:00Z",
  updatedAt: "2026-02-23T10:00:00Z",
  isFuture: false,
  title: "API Arch View",
  category: "arch_view",
};

/** A future view — sidebar rows carry no implement affordance anymore */
const futureViewDoc: DocNode = {
  id: "doc-future-view-1",
  name: "new-feature.pm_view.md",
  docType: "pm_view",
  filePath: "views/future_views/new-feature.pm_view.md",
  linkedNodeCount: 0,
  lastUpliftedAt: null,
  lastDownliftedAt: null,
  contentUpdatedAt: "2026-02-22T10:00:00Z",
  createdAt: "2026-02-22T10:00:00Z",
  updatedAt: "2026-02-22T10:00:00Z",
  isFuture: true,
  title: "New Feature Vision",
  category: "pm_view",
};

/** A test_view via category field (docType differs) */
const testViewViaCategoryDoc: DocNode = {
  id: "doc-test-view-2",
  name: "e2e-tests.md",
  docType: "note",
  filePath: "views/test_views/e2e-tests.md",
  linkedNodeCount: 1,
  lastUpliftedAt: null,
  lastDownliftedAt: null,
  contentUpdatedAt: "2026-02-26T10:00:00Z",
  createdAt: "2026-02-26T10:00:00Z",
  updatedAt: "2026-02-26T10:00:00Z",
  isFuture: false,
  title: "E2E Tests View",
  category: "test_view",
};

function defaultSidebarProps(overrides: Record<string, unknown> = {}) {
  return {
    isOpen: true,
    width: 280,
    workspaceId: "test-workspace",
    organization: "g5labs",
    branches: [],
    tasks: [],
    docs: [] as DocNode[],
    selectedDocId: null as string | null,
    selectedTaskId: null as string | null,
    onSelectDoc: vi.fn(),
    onSelectFile: vi.fn(),
    onSelectTask: vi.fn(),
    searchQuery: "",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Expand the views/<subfolder> chain inside the All Docs section. */
async function revealViewRows(subfolders: string[]): Promise<HTMLElement> {
  const user = userEvent.setup();
  activateSidebarSection("sidebar-all-docs-tab");
  const allDocsTab = screen.getByTestId("sidebar-all-docs-tab");
  await waitFor(() => {
    expect(within(allDocsTab).getByText("views")).toBeInTheDocument();
  });
  await user.click(within(allDocsTab).getByText("views"));
  for (const folder of subfolders) {
    await waitFor(() => {
      expect(within(allDocsTab).getByText(folder)).toBeInTheDocument();
    });
    await user.click(within(allDocsTab).getByText(folder));
  }
  return allDocsTab;
}

function expectNoInlineSyncControls() {
  expect(
    screen.queryByLabelText("Sync linked docs to test view"),
  ).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Generate test code")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Sync uplift")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Sync downlift")).not.toBeInTheDocument();
  expect(screen.queryByTitle(/Sync:/i)).not.toBeInTheDocument();
}

// =============================================================================
// Tests
// =============================================================================

describe("View sidebar rows folded into All Docs", () => {
  describe("test_view documents", () => {
    it("renders test views in All Docs without inline sync controls", async () => {
      const props = defaultSidebarProps({ docs: [testViewDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["test_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("auth-module.test_view.md"),
        ).toBeInTheDocument();
      });
      expectNoInlineSyncControls();
    });

    it("treats category=test_view the same way", async () => {
      const props = defaultSidebarProps({ docs: [testViewViaCategoryDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["test_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("e2e-tests.md"),
        ).toBeInTheDocument();
      });
      expectNoInlineSyncControls();
    });

    it("shows the view file name and full path preview", async () => {
      const props = defaultSidebarProps({ docs: [testViewDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["test_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("auth-module.test_view.md"),
        ).toBeInTheDocument();
      });
      expect(
        within(allDocsTab).getByText(
          "views/test_views/auth-module.test_view.md",
        ),
      ).toBeInTheDocument();
    });
  });

  describe("non-test view documents", () => {
    it("renders PM views in All Docs without inline sync controls", async () => {
      const props = defaultSidebarProps({ docs: [pmViewDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["pm_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("billing.pm_view.md"),
        ).toBeInTheDocument();
      });
      expectNoInlineSyncControls();
    });

    it("renders arch views in All Docs without inline sync controls", async () => {
      const props = defaultSidebarProps({ docs: [archViewDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["arch_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("api.arch_view.md"),
        ).toBeInTheDocument();
      });
      expectNoInlineSyncControls();
    });
  });

  describe("future view documents", () => {
    it("renders future views without sync or implement affordances", async () => {
      const props = defaultSidebarProps({ docs: [futureViewDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["future_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("new-feature.pm_view.md"),
        ).toBeInTheDocument();
      });

      // No sync buttons, and the sidebar implement affordance is retired
      // (left-sidebar D36) — implement lives in the main doc workflow.
      expectNoInlineSyncControls();
      expect(screen.queryByLabelText("Implement view")).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("tree-view-implement"),
      ).not.toBeInTheDocument();
    });
  });

  describe("mixed documents", () => {
    it("renders mixed view types in All Docs without any inline sync controls", async () => {
      const props = defaultSidebarProps({ docs: [testViewDoc, pmViewDoc] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      const allDocsTab = await revealViewRows(["test_views", "pm_views"]);

      await waitFor(() => {
        expect(
          within(allDocsTab).getByText("auth-module.test_view.md"),
        ).toBeInTheDocument();
        expect(
          within(allDocsTab).getByText("billing.pm_view.md"),
        ).toBeInTheDocument();
      });
      expectNoInlineSyncControls();
    });
  });
});
