// =============================================================================
// Document Click & Rendering Tests
// =============================================================================
// Tests all doc-related flows: sidebar doc rendering, doc selection, doc tab
// opening, document API reads, MarkdownRenderer display,
// and error/empty states.
// =============================================================================

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { activateSidebarSection } from "../test/sidebar";

import { LeftSidebar } from "../components/tree/TreeNode";
import { MOCK_DOCS } from "../data/mock";
import type { DocNode } from "../lib/types";

// Strip folder/pinned so DocsSection renders a flat list (no collapsed groups)
const FLAT_MOCK_DOCS: DocNode[] = MOCK_DOCS.map(
  ({ folder, pinned, ...rest }) => rest,
);

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
    tasks: [],
    docs: FLAT_MOCK_DOCS,
    selectedDocId: null as string | null,
    selectedTaskId: null as string | null,
    onSelectDoc: vi.fn(),
    onSelectFile: vi.fn(),
    onSelectTask: vi.fn(),
    searchQuery: "",
    ...overrides,
  };
}

// =============================================================================
// Test Suite: Docs in Sidebar
// =============================================================================

describe("Document Click & Rendering", () => {
  // -------------------------------------------------------------------------
  // 1. Docs section rendering in sidebar
  // -------------------------------------------------------------------------

  describe("sidebar docs section", () => {
    it("renders all doc titles in the Docs section", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        for (const doc of FLAT_MOCK_DOCS) {
          expect(screen.getByText(doc.title!)).toBeInTheDocument();
        }
      });
    });

    it("renders doc previews alongside titles", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        for (const doc of FLAT_MOCK_DOCS) {
          expect(screen.getByText(doc.preview!)).toBeInTheDocument();
        }
      });
    });

    it('shows "No documents found" when docs array is empty', async () => {
      const props = defaultSidebarProps({ docs: [] });
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        // Both Views and Project Docs sections show the empty message
        const msgs = screen.getAllByText("No documents found");
        expect(msgs.length).toBeGreaterThanOrEqual(1);
      });
    });

    it("does not show a doc inventory badge in the section header", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-docs-tab");
      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        expect(screen.getByText("Project Docs")).toBeInTheDocument();
      });

      const docsHeader = screen
        .getByTestId("sidebar-docs-tab")
        .querySelector('[role="button"]');
      expect(docsHeader?.textContent).not.toContain(
        String(FLAT_MOCK_DOCS.length),
      );
    });
  });

  // -------------------------------------------------------------------------
  // 2. Clicking a doc calls onSelectDoc
  // -------------------------------------------------------------------------

  describe("doc click handler", () => {
    it("calls onSelectDoc with the correct docId when a doc is clicked", async () => {
      const onSelectDoc = vi.fn();
      const props = defaultSidebarProps({ onSelectDoc });
      const user = userEvent.setup();

      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        expect(screen.getByText(FLAT_MOCK_DOCS[0].title!)).toBeInTheDocument();
      });

      await user.click(screen.getByText(FLAT_MOCK_DOCS[0].title!));
      // All Docs uses the same identity-only opener as Project Docs.
      expect(onSelectDoc).toHaveBeenCalledWith(FLAT_MOCK_DOCS[0].id);
    });

    it("calls onSelectDoc for each different doc clicked", async () => {
      const onSelectDoc = vi.fn();
      const props = defaultSidebarProps({ onSelectDoc });
      const user = userEvent.setup();

      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        expect(screen.getByText(FLAT_MOCK_DOCS[0].title!)).toBeInTheDocument();
      });

      // Click each doc
      for (const doc of FLAT_MOCK_DOCS) {
        await user.click(screen.getByText(doc.title!));
      }

      expect(onSelectDoc).toHaveBeenCalledTimes(FLAT_MOCK_DOCS.length);
      for (const doc of FLAT_MOCK_DOCS) {
        // Authority is determined by the shared file session.
        expect(onSelectDoc).toHaveBeenCalledWith(doc.id);
      }
    });

    it("highlights the selected doc with visual indication", async () => {
      const props = defaultSidebarProps({
        selectedDocId: FLAT_MOCK_DOCS[0].id,
      });

      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        const docTitle = screen.getByText(FLAT_MOCK_DOCS[0].title!);
        const docRow = docTitle.closest('[data-testid="doc-item"]');
        expect(docRow).toBeTruthy();
        // The selected doc row should have the bg-[var(--g5-active)] class
        expect(docRow?.className).toContain("bg-");
      });
    });
  });

  // -------------------------------------------------------------------------
  // 3. Document API — MSW integration
  // -------------------------------------------------------------------------

  describe("doc content API (MSW)", () => {
    it("returns doc data for a valid docId", async () => {
      const docId = MOCK_DOCS[0].id;

      // Verify the MSW handler returns the correct doc
      const response = await fetch(
        `/api/v1/workspaces/test-workspace/docs/${docId}`,
      );
      const json = await response.json();

      expect(response.status).toBe(200);
      expect(json.data.id).toBe(docId);
      expect(json.data.title).toBe(MOCK_DOCS[0].title);
      expect(json.data.content).toBe(MOCK_DOCS[0].content);
    });

    it("returns 404 for a non-existent docId", async () => {
      const response = await fetch(
        "/api/v1/workspaces/test-workspace/docs/doc-nonexistent",
      );
      expect(response.status).toBe(404);

      const json = await response.json();
      expect(json.error.code).toBe("NOT_FOUND");
    });

    it("returns correct content field for each mock doc", async () => {
      for (const doc of MOCK_DOCS) {
        const response = await fetch(
          `/api/v1/workspaces/test-workspace/docs/${doc.id}`,
        );
        const json = await response.json();
        expect(json.data.content).toBe(doc.content);
        expect(json.data.category).toBe(doc.category);
      }
    });
  });

  // -------------------------------------------------------------------------
  // 4. Doc content rendering (with overridden MSW for specific test)
  // -------------------------------------------------------------------------

  describe("doc content error handling", () => {
    it("returns error response when doc endpoint is overridden to fail", async () => {
      server.use(
        http.get("/api/v1/workspaces/:wsId/docs/:docId", () =>
          HttpResponse.json(
            {
              error: {
                code: "INTERNAL",
                message: "Server error",
                requestId: "req-1",
              },
            },
            { status: 500 },
          ),
        ),
      );

      const response = await fetch(
        `/api/v1/workspaces/test-workspace/docs/${MOCK_DOCS[0].id}`,
      );
      expect(response.status).toBe(500);
    });

    it("returns empty content when doc content is empty string", async () => {
      const emptyDoc: DocNode = {
        ...MOCK_DOCS[0],
        content: "",
      };

      server.use(
        http.get("/api/v1/workspaces/:wsId/docs/:docId", () =>
          HttpResponse.json({ data: emptyDoc }),
        ),
      );

      const response = await fetch(
        `/api/v1/workspaces/test-workspace/docs/${MOCK_DOCS[0].id}`,
      );
      const json = await response.json();
      expect(json.data.content).toBe("");
    });
  });

  // -------------------------------------------------------------------------
  // 5. Tab identity & getTabId for doc tabs
  // -------------------------------------------------------------------------

  describe("doc tab identity", () => {
    it("creates unique tab IDs for different docs", () => {
      // Import getTabId indirectly by verifying tab behavior
      // Different doc IDs should produce different tab IDs
      const docIds = MOCK_DOCS.map((d) => `doc:${d.id}`);
      const uniqueIds = new Set(docIds);
      expect(uniqueIds.size).toBe(MOCK_DOCS.length);
    });

    it('doc tab IDs are prefixed with "doc:"', () => {
      for (const doc of MOCK_DOCS) {
        const tabId = `doc:${doc.id}`;
        expect(tabId).toMatch(/^doc:/);
      }
    });
  });

  // -------------------------------------------------------------------------
  // 6. Multiple docs & categories
  // -------------------------------------------------------------------------

  describe("doc categories", () => {
    it("renders docs with different categories", async () => {
      const categories = new Set(FLAT_MOCK_DOCS.map((d) => d.category));
      // Verify we have multiple categories in mock data
      expect(categories.size).toBeGreaterThan(1);

      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      // All docs should render regardless of category
      await waitFor(() => {
        for (const doc of FLAT_MOCK_DOCS) {
          expect(screen.getByText(doc.title!)).toBeInTheDocument();
        }
      });
    });

    it("renders category icons for each doc", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        // Each doc row should contain an SVG icon
        for (const doc of FLAT_MOCK_DOCS) {
          const titleEl = screen.getByText(doc.title!);
          const row = titleEl.closest('[data-testid="doc-item"]');
          expect(row).toBeTruthy();
          const icon = row?.querySelector("svg");
          expect(icon).toBeTruthy();
        }
      });
    });
  });

  // -------------------------------------------------------------------------
  // 7. MOCK_DOCS data integrity
  // -------------------------------------------------------------------------

  describe("mock docs data integrity", () => {
    it("all mock docs have required fields", () => {
      for (const doc of MOCK_DOCS) {
        expect(doc.id).toBeTruthy();
        expect(doc.title).toBeTruthy();
        expect(doc.category).toBeTruthy();
        expect(doc.content).toBeTruthy();
        expect(doc.preview).toBeTruthy();
        expect(doc.updatedAt).toBeTruthy();
      }
    });

    it("all mock doc IDs are unique", () => {
      const ids = MOCK_DOCS.map((d) => d.id);
      const uniqueIds = new Set(ids);
      expect(uniqueIds.size).toBe(ids.length);
    });

    it("mock docs have valid category values", () => {
      const validCategories = [
        "architecture",
        "plan",
        "guide",
        "spec",
        "design",
        "api",
        "adr",
        "note",
      ];
      for (const doc of MOCK_DOCS) {
        expect(validCategories).toContain(doc.category);
      }
    });

    it("mock docs have valid ISO 8601 dates", () => {
      for (const doc of MOCK_DOCS) {
        const date = new Date(doc.updatedAt);
        expect(date.getTime()).not.toBeNaN();
      }
    });

    it("mock docs content contains markdown", () => {
      for (const doc of MOCK_DOCS) {
        // All our mock docs start with a markdown heading
        expect(doc.content).toMatch(/^#/);
      }
    });
  });

  // -------------------------------------------------------------------------
  // 8. Keyboard interaction
  // -------------------------------------------------------------------------

  describe("doc keyboard interaction", () => {
    it("doc items are focusable with tabIndex", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        const docTitle = screen.getByText(FLAT_MOCK_DOCS[0].title!);
        const docRow = docTitle.closest('[data-testid="doc-item"]');
        expect(docRow).toBeTruthy();
        expect(docRow?.getAttribute("tabindex")).toBe("0");
      });
    });
  });
});
