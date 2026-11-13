// =============================================================================
// Sidebar Integration Tests
// =============================================================================
// Tests the left sidebar component including workspace list loading, ontology
// tree rendering, navigation, and active state management.
// =============================================================================

import { useState, type ComponentProps } from "react";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { createMockTask } from "../test/fixtures";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { activateSidebarSection } from "../test/sidebar";
import type { WorkspaceUiCapabilities } from "../lib/capabilities";
import { WorkspaceNamesProvider } from "../hooks/useWorkspaceNames";

// Import component under test
import { LeftSidebar as WorkspaceSidebar } from "./tree/TreeNode";
import { SourceControlProvider } from "./source-control/SourceControlBrowser";
import { WorkspaceLiveGateProvider } from "../lib/workspaceLiveGate";
function LeftSidebar(props: ComponentProps<typeof WorkspaceSidebar>) {
  return (
    <SourceControlProvider workspaceId={props.workspaceId}>
      <WorkspaceSidebar {...props} />
    </SourceControlProvider>
  );
}
function StoppedSourceControlSidebar(
  props: ComponentProps<typeof WorkspaceSidebar>,
) {
  return (
    <WorkspaceLiveGateProvider
      value={{
        kind: "stopped",
        live: false,
        owner: true,
        canStart: true,
        canWrite: false,
        reviewCopy: false,
        reviewIdentityKnown: true,
        workspaceNoun: "the workspace",
      }}
    >
      <LeftSidebar {...props} />
    </WorkspaceLiveGateProvider>
  );
}
const INTENT_LOG_VIEWER_CAPABILITIES: WorkspaceUiCapabilities = {
  cloudWorkspaceWorkflows: true,
  intentLogViewer: true,
};

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

// =============================================================================
// Test Suite
// =============================================================================

describe("LeftSidebar", () => {
  describe("loaded states", () => {
    it("renders all default sidebar section headers", async () => {
      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      // The sidebar renders six stacked top-level sections.
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
    });

    it("keeps the Diff Graph entry out of the repository checklist", () => {
      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });
      activateSidebarSection("sidebar-source-control-tab");
      expect(
        screen.queryByTestId("source-control-change-graph-button"),
      ).not.toBeInTheDocument();
    });

    it("renders no commit/discard mirror when the host supplies no cluster inputs", () => {
      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });
      activateSidebarSection("sidebar-source-control-tab");
      expect(
        screen.queryByTestId("source-control-commit-button"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("source-control-discard-button"),
      ).not.toBeInTheDocument();
    });

    it("renders project doc folders collapsed across rerenders", async () => {
      const user = userEvent.setup();
      const initialDocs = [
        {
          id: "doc-existing",
          name: "existing-design.md",
          docType: "design",
          filePath: "designs/existing-design.md",
          folder: "designs",
        },
      ];
      const addedDoc = {
        id: "doc-plain",
        name: "plain-doc.md",
        docType: "design",
        filePath: "docs/archive/plain-doc.md",
        folder: "docs/archive",
      };
      const props = defaultSidebarProps({ docs: initialDocs });
      const view = renderApp(<WorkspaceSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        expect(screen.getByText("designs")).toBeInTheDocument();
      });
      const designsFolder = screen.getByText("designs").closest("button");
      if (!designsFolder) throw new Error("designs folder not found");
      expect(designsFolder).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText("existing-design.md")).not.toBeInTheDocument();
      await user.click(designsFolder);
      expect(screen.getByText("existing-design.md")).toBeInTheDocument();

      view.rerender(
        <WorkspaceSidebar
          {...defaultSidebarProps({ docs: [...initialDocs, addedDoc] })}
        />,
      );

      activateSidebarSection("sidebar-all-docs-tab");

      await waitFor(() => {
        expect(screen.getByText("archive")).toBeInTheDocument();
      });
      const archiveFolder = screen.getByText("archive").closest("button");
      if (!archiveFolder) throw new Error("archive folder not found");
      expect(archiveFolder).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText("plain-doc.md")).not.toBeInTheDocument();
      await user.click(archiveFolder);
      expect(screen.getByText("plain-doc.md")).toBeInTheDocument();
    });
  });

  describe("section order defaults", () => {
    function sectionTabIds(): string[] {
      return Array.from(
        screen
          .getByTestId("left-sidebar")
          .querySelectorAll('[data-testid^="sidebar-"][data-testid$="-tab"]'),
      ).map((el) => el.getAttribute("data-testid")!);
    }

    it("places Core Ontology first in the stacked sidebar by default", async () => {
      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(sectionTabIds()).toEqual([
        "sidebar-ontology-tab",
        "sidebar-docs-tab",
        "sidebar-all-docs-tab",
        "sidebar-source-control-tab",
        "sidebar-tasks-tab",
        "sidebar-files-tab",
      ]);
    });

    it("migrates a persisted legacy default order to the ontology-first default", async () => {
      localStorage.setItem(
        "g5-sidebar-order-test-workspace",
        JSON.stringify([
          "project-docs",
          "views",
          "issues",
          "ontology",
          "tasks",
          "files",
        ]),
      );

      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(sectionTabIds()).toEqual([
        "sidebar-ontology-tab",
        "sidebar-docs-tab",
        "sidebar-all-docs-tab",
        "sidebar-source-control-tab",
        "sidebar-tasks-tab",
        "sidebar-files-tab",
      ]);
      // The migrated order is re-persisted with the current canonical default
      // (Views and Explainers are both retired: explainers are Project Docs).
      expect(
        JSON.parse(localStorage.getItem("g5-sidebar-order-test-workspace")!),
      ).toEqual([
        "ontology",
        "project-docs",
        "all-docs",
        "source-control",
        "tasks",
        "files",
        "issues",
        "guided",
      ]);
    });

    it("migrates the outgoing default that still lists the retired issues section", async () => {
      localStorage.setItem(
        "g5-sidebar-order-test-workspace",
        JSON.stringify([
          "ontology",
          "project-docs",
          "explainers",
          "all-docs",
          "source-control",
          "issues",
          "tasks",
          "files",
        ]),
      );

      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(
        JSON.parse(localStorage.getItem("g5-sidebar-order-test-workspace")!),
      ).toEqual([
        "ontology",
        "project-docs",
        "all-docs",
        "source-control",
        "tasks",
        "files",
        "issues",
        "guided",
      ]);
    });

    it("preserves a genuinely customised persisted order", async () => {
      localStorage.setItem(
        "g5-sidebar-order-test-workspace",
        JSON.stringify([
          "files",
          "project-docs",
          "views",
          "issues",
          "ontology",
          "tasks",
        ]),
      );

      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });

      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(sectionTabIds()[0]).toBe("sidebar-files-tab");
    });
  });

  describe("workspace switch collapse state", () => {
    it("resets accordion expansion when the workspace changes without remounting", async () => {
      const view = renderApp(
        <WorkspaceSidebar {...defaultSidebarProps({ workspaceId: "ws-a" })} />,
        { route: "/org-g5labs/test-workspace" },
      );

      activateSidebarSection("sidebar-files-tab");
      await waitFor(() => {
        expect(
          within(screen.getByTestId("sidebar-files-tab"))
            .getByRole("button", { name: "Files" })
            .getAttribute("aria-expanded"),
        ).toBe("true");
      });

      // Switch to workspace B on the SAME sidebar instance (no remount).
      view.rerender(
        <WorkspaceSidebar {...defaultSidebarProps({ workspaceId: "ws-b" })} />,
      );

      // Workspace B starts calm: the files section does not inherit A's
      // expanded accordion state.
      await waitFor(() => {
        expect(
          within(screen.getByTestId("sidebar-files-tab"))
            .getByRole("button", { name: "Files" })
            .getAttribute("aria-expanded"),
        ).toBe("false");
      });
    });

    it("re-derives section order when the workspace changes without overwriting the next workspace", async () => {
      const user = userEvent.setup();
      const workspaceAOrder = [
        "project-docs",
        "views",
        "all-docs",
        "ontology",
        "issues",
        "tasks",
        "files",
      ];
      const workspaceBOrder = [
        "files",
        "all-docs",
        "ontology",
        "project-docs",
        "views",
        "issues",
        "tasks",
      ];
      localStorage.setItem(
        "g5-sidebar-order-ws-a",
        JSON.stringify(workspaceAOrder),
      );
      localStorage.setItem(
        "g5-sidebar-order-ws-b",
        JSON.stringify(workspaceBOrder),
      );

      const sectionTabIds = () =>
        Array.from(
          screen
            .getByTestId("left-sidebar")
            .querySelectorAll('[data-testid^="sidebar-"][data-testid$="-tab"]'),
        ).map((el) => el.getAttribute("data-testid")!);

      function SidebarHarness() {
        const [workspaceId, setWorkspaceId] = useState("ws-a");
        return (
          <>
            <button onClick={() => setWorkspaceId("ws-b")}>
              Switch workspace
            </button>
            <LeftSidebar {...defaultSidebarProps({ workspaceId })} />
          </>
        );
      }

      renderApp(<SidebarHarness />, { route: "/org-g5labs/test-workspace" });

      await waitFor(() => {
        expect(sectionTabIds()[0]).toBe("sidebar-docs-tab");
      });

      await user.click(
        screen.getByRole("button", { name: "Switch workspace" }),
      );

      await waitFor(() => {
        expect(sectionTabIds()[0]).toBe("sidebar-files-tab");
      });
      expect(
        JSON.parse(localStorage.getItem("g5-sidebar-order-ws-a")!),
      ).toEqual([
        "project-docs",
        "all-docs",
        "source-control",
        "ontology",
        "issues",
        "guided",
        "tasks",
        "files",
      ]);
      expect(
        JSON.parse(localStorage.getItem("g5-sidebar-order-ws-b")!),
      ).toEqual([
        "files",
        "all-docs",
        "source-control",
        "ontology",
        "project-docs",
        "issues",
        "guided",
        "tasks",
      ]);
    });
  });

  describe("worker admin button", () => {
    it("does not render worker-admin controls in the sidebar", async () => {
      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(
        screen.queryByTestId("sidebar-worker-admin-button"),
      ).not.toBeInTheDocument();
    });

    it("keeps worker-admin controls out of the sidebar even when callback props exist", async () => {
      const onOpenWorkerAdmin = vi.fn();
      renderApp(
        <LeftSidebar {...defaultSidebarProps({ onOpenWorkerAdmin })} />,
        { route: "/org-g5labs/test-workspace" },
      );
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(
        screen.queryByTestId("sidebar-worker-admin-button"),
      ).not.toBeInTheDocument();
      expect(onOpenWorkerAdmin).not.toHaveBeenCalled();
    });
  });

  describe("workspace settings button", () => {
    it("does not render the settings button by default", async () => {
      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(
        screen.queryByTestId("sidebar-settings-button"),
      ).not.toBeInTheDocument();
    });

    it("keeps settings controls out of the sidebar even when callback props exist", async () => {
      const onOpenWorkspaceSettings = vi.fn();
      renderApp(
        <LeftSidebar {...defaultSidebarProps({ onOpenWorkspaceSettings })} />,
        { route: "/org-g5labs/test-workspace" },
      );
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(
        screen.queryByTestId("sidebar-settings-button"),
      ).not.toBeInTheDocument();
      expect(onOpenWorkspaceSettings).not.toHaveBeenCalled();
    });
  });

  describe("intent log button", () => {
    it("does not render the intent-log button by default", async () => {
      renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
        route: "/org-g5labs/test-workspace",
      });
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("open-intent-log")).not.toBeInTheDocument();
    });

    it("does not render the intent-log button when the feature flag is off", async () => {
      renderApp(
        <LeftSidebar
          {...defaultSidebarProps({
            onOpenIntentLog: vi.fn(),
            capabilities: {
              cloudWorkspaceWorkflows: true,
              intentLogViewer: false,
            },
          })}
        />,
        { route: "/org-g5labs/test-workspace" },
      );
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("open-intent-log")).not.toBeInTheDocument();
    });

    it("keeps intent-log controls out of the sidebar even when callback props exist", async () => {
      const onOpenIntentLog = vi.fn();
      renderApp(
        <LeftSidebar
          {...defaultSidebarProps({
            onOpenIntentLog,
            capabilities: INTENT_LOG_VIEWER_CAPABILITIES,
          })}
        />,
        { route: "/org-g5labs/test-workspace" },
      );
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("open-intent-log")).not.toBeInTheDocument();
      expect(onOpenIntentLog).not.toHaveBeenCalled();
    });

    it("does not render an intent-log sidebar button when its tab is active elsewhere", async () => {
      renderApp(
        <LeftSidebar
          {...defaultSidebarProps({
            onOpenIntentLog: vi.fn(),
            capabilities: INTENT_LOG_VIEWER_CAPABILITIES,
            activeRailAction: "intent_log",
          })}
        />,
        { route: "/org-g5labs/test-workspace" },
      );
      await waitFor(() => {
        expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("open-intent-log")).not.toBeInTheDocument();
    });
  });

  describe("error states", () => {
    it("renders sidebar even when workspace endpoint returns 500", async () => {
      server.use(
        http.get("/api/v1/orgs/:orgId/workspaces", () =>
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

      const props = defaultSidebarProps();
      renderApp(<LeftSidebar {...props} />, {
        route: "/org-g5labs/test-workspace",
      });

      activateSidebarSection("sidebar-files-tab");

      // Sidebar should still render its structure even if workspace data fails
      await waitFor(() => {
        expect(screen.getByText("Files")).toBeInTheDocument();
      });
    });
  });
});

// =============================================================================
// Display modes (left-sidebar D38)
// =============================================================================

describe("LeftSidebar display modes (left-sidebar D38)", () => {
  it("renders the stacked panel with no rail in the default mode, with and without the prop", () => {
    const { unmount } = renderApp(<LeftSidebar {...defaultSidebarProps()} />, {
      route: "/org-g5labs/test-workspace",
    });
    expect(screen.getByTestId("sidebar-files-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-rail")).not.toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-rail-files")).not.toBeInTheDocument();
    unmount();

    renderApp(
      <LeftSidebar {...defaultSidebarProps({ displayMode: "expanded" })} />,
      { route: "/org-g5labs/test-workspace" },
    );
    expect(screen.getByTestId("sidebar-files-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-rail")).not.toBeInTheDocument();
  });

  // ui g5.ui.sidebar-default: `collapse-to-rail` is the full sidebar whose
  // collapsed state is the icon rail with an expand control.
  it("collapse-to-rail renders the stacked panel while open and the icon rail with an expand control while collapsed", async () => {
    function Harness() {
      const [open, setOpen] = useState(true);
      return (
        <LeftSidebar
          {...defaultSidebarProps({
            displayMode: "collapse-to-rail",
            isOpen: open,
            onExpand: () => setOpen(true),
            onCollapse: () => setOpen(false),
          })}
        />
      );
    }
    renderApp(<Harness />, { route: "/org-g5labs/test-workspace" });
    const user = userEvent.setup();
    // Open: the full sidebar, with its collapse control and no rail.
    expect(screen.getByTestId("sidebar-files-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-rail")).not.toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-expand")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("sidebar-collapse"));

    // Collapsed: the icon rail with an expand control, no section content,
    // no collapse control and no overlay.
    const rail = screen.getByTestId("sidebar-rail");
    expect(rail).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-files")).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-docs")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-files-tab")).not.toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-collapse")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand sidebar" }));
    expect(screen.getByTestId("sidebar-files-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-rail")).not.toBeInTheDocument();

    // A section icon expands the panel on that section.
    await user.click(screen.getByTestId("sidebar-collapse"));
    await user.click(screen.getByTestId("sidebar-rail-docs"));
    expect(screen.queryByTestId("sidebar-rail")).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("sidebar-docs-tab")).getByRole("button", {
        name: /docs/i,
      }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("collapse-to-rail renders nothing while collapsed without an expand handler", () => {
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          displayMode: "collapse-to-rail",
          isOpen: false,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    expect(screen.queryByTestId("left-sidebar")).not.toBeInTheDocument();
  });

  it("rail mode renders one icon per visible section with no mounted content", () => {
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          displayMode: "rail",
          capabilities: {
            cloudWorkspaceWorkflows: false,
            intentLogViewer: false,
          },
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    expect(screen.getByTestId("sidebar-rail")).toBeInTheDocument();
    const filesRail = screen.getByTestId("sidebar-rail-files");
    expect(filesRail).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByTestId("sidebar-rail-docs")).toBeInTheDocument();
    // Capability-hidden sections have no rail icon, and there is no
    // Explainers section at all (explainers are Project Docs).
    expect(
      screen.queryByTestId("sidebar-rail-explainers"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-rail-tasks")).not.toBeInTheDocument();
    // No stacked section content is mounted while the overlay is closed.
    expect(screen.queryByTestId("sidebar-files-tab")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();
  });

  it("rail mode opens a single-section overlay with pinned tasks, switches sections, and toggles closed", async () => {
    const user = userEvent.setup();
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          displayMode: "rail",
          capabilities: INTENT_LOG_VIEWER_CAPABILITIES,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    await user.click(screen.getByTestId("sidebar-rail-files"));
    await waitFor(() =>
      expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument(),
    );
    // Single-section overlay: exactly the activated section, expanded —
    // no other stacked sections.
    const filesTab = screen.getByTestId("sidebar-files-tab");
    expect(
      within(filesTab).getByRole("button", { name: "Files" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByTestId("sidebar-docs-tab")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("sidebar-source-control-tab"),
    ).not.toBeInTheDocument();
    // Task Queue is always pinned at the bottom in addition to the section.
    const pinnedTasks = screen.getByTestId("sidebar-rail-pinned-tasks");
    expect(
      within(pinnedTasks).getByTestId("sidebar-tasks-tab"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-files")).toHaveAttribute(
      "aria-expanded",
      "true",
    );

    // A different icon switches the overlay to that section.
    await user.click(screen.getByTestId("sidebar-rail-docs"));
    await waitFor(() =>
      expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("sidebar-files-tab")).not.toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-pinned-tasks")).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-files")).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    // Activating tasks itself shows only the Task Queue (no pinned copy).
    await user.click(screen.getByTestId("sidebar-rail-tasks"));
    await waitFor(() =>
      expect(screen.getByTestId("sidebar-tasks-tab")).toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("sidebar-rail-pinned-tasks"),
    ).not.toBeInTheDocument();

    // Activating the active section's own icon closes the overlay.
    await user.click(screen.getByTestId("sidebar-rail-tasks"));
    await waitFor(() =>
      expect(
        screen.queryByTestId("sidebar-rail-overlay"),
      ).not.toBeInTheDocument(),
    );

    // Reopen, then Escape closes.
    await user.click(screen.getByTestId("sidebar-rail-files"));
    await waitFor(() =>
      expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument(),
    );
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(
        screen.queryByTestId("sidebar-rail-overlay"),
      ).not.toBeInTheDocument(),
    );
  });

  it("rail mode opens the overlay for an external activateSectionRequest", async () => {
    const user = userEvent.setup();
    // Stateful harness: the shell delivers activateSectionRequest as a prop
    // change (e.g. the top-bar task progress indicator revealing the Task
    // Queue).
    function RailActivateHarness() {
      const [request, setRequest] = useState<{
        section: "tasks";
        nonce: number;
      } | null>(null);
      return (
        <>
          <button
            type="button"
            data-testid="fire-activate-request"
            onClick={() => setRequest({ section: "tasks", nonce: 1 })}
          >
            fire
          </button>
          <LeftSidebar
            {...defaultSidebarProps({
              displayMode: "rail",
              capabilities: INTENT_LOG_VIEWER_CAPABILITIES,
              activateSectionRequest: request,
            })}
          />
        </>
      );
    }
    renderApp(<RailActivateHarness />, {
      route: "/org-g5labs/test-workspace",
    });
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByTestId("fire-activate-request"));
    await waitFor(() =>
      expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("sidebar-tasks-tab")).toBeInTheDocument();
    expect(
      screen.queryByTestId("sidebar-rail-pinned-tasks"),
    ).not.toBeInTheDocument();
  });

  it("expand-on-hover mode opens the same single-section overlay on hover and closes on leave", async () => {
    const user = userEvent.setup();
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          displayMode: "expand-on-hover",
          capabilities: INTENT_LOG_VIEWER_CAPABILITIES,
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    expect(screen.getByTestId("sidebar-rail")).toBeInTheDocument();
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();

    // Hovering a rail icon opens the single-section overlay on THAT
    // section, expanded, with the pinned Task Queue — no other sections.
    await user.hover(screen.getByTestId("sidebar-rail-files"));
    expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument();
    const filesTab = screen.getByTestId("sidebar-files-tab");
    expect(
      within(filesTab).getByRole("button", { name: "Files" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByTestId("sidebar-docs-tab")).not.toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-pinned-tasks")).toBeInTheDocument();

    // Hovering another rail icon switches the overlay to that section.
    await user.hover(screen.getByTestId("sidebar-rail-docs"));
    expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-files-tab")).not.toBeInTheDocument();

    // Clicking the active section's own icon closes the overlay.
    await user.click(screen.getByTestId("sidebar-rail-docs"));
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();

    // Re-hover reopens on the sticky last-active section; leave closes.
    await user.unhover(screen.getByTestId("left-sidebar"));
    await user.hover(screen.getByTestId("left-sidebar"));
    expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
    await user.unhover(screen.getByTestId("left-sidebar"));
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();
  });

  it("expand-on-hover mode opens a file clicked in the overlay while a sidebar control holds focus", async () => {
    const user = userEvent.setup();
    const onSelectFile = vi.fn();
    server.use(
      http.get("/api/v1/workspaces/test-workspace/repos/repo-code/files", () =>
        HttpResponse.json({
          data: [{ name: "README.md", path: "README.md", type: "file" }],
        }),
      ),
    );
    renderApp(
      <LeftSidebar
        {...defaultSidebarProps({
          displayMode: "expand-on-hover",
          onSelectFile,
          branches: [
            {
              workspaceId: "test-workspace",
              repoId: "repo-code",
              repoName: "code",
              branchName: "feature/code",
              trackingMode: "tracking",
              headCommit: "a".repeat(40),
              divergence: 0,
              behindBase: 0,
              hasDraft: false,
              dirtyFileCount: 0,
            },
          ],
        })}
      />,
      { route: "/org-g5labs/test-workspace" },
    );

    // A click on the hovered Files icon toggles the overlay closed and leaves
    // the icon focused, as a folder's chevron or a section header would be;
    // the next hover reopens the overlay with that focus still inside.
    await user.click(screen.getByTestId("sidebar-rail-files"));
    expect(screen.getByTestId("sidebar-rail-files")).toHaveFocus();
    expect(
      screen.queryByTestId("sidebar-rail-overlay"),
    ).not.toBeInTheDocument();
    await user.unhover(screen.getByTestId("left-sidebar"));
    await user.hover(screen.getByTestId("sidebar-rail-files"));
    expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument();
    expect(screen.getByTestId("sidebar-rail-files")).toHaveFocus();

    // The repository root and its file rows are not focusable: a browser's
    // press on one blurs the focused icon with no related target before the
    // click lands, and the overlay must stay for the click to reach the row.
    // The press is replayed by hand: user-event's pointer moves carry no
    // related target, so React would report a leave of the whole sidebar.
    const press = async (element: HTMLElement) => {
      fireEvent.pointerDown(element);
      fireEvent.mouseDown(element);
      await act(async () => {
        (document.activeElement as HTMLElement | null)?.blur();
      });
      fireEvent.pointerUp(element);
      fireEvent.mouseUp(element);
      fireEvent.click(element);
    };
    await press(screen.getByText("code"));
    expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument();
    await press(await screen.findByText("README.md"));
    expect(onSelectFile).toHaveBeenCalledWith("repo-code", "README.md");
    expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument();
  });
});

it.each(["default", "workbench"] as const)(
  "uses repository selection without history scopes in the %s sidebar",
  async (preset) => {
    const props = defaultSidebarProps({
      sectionOrderPreset: preset === "default" ? undefined : "workbench",
    });
    server.use(
      http.get("/api/v1/workspaces/test-workspace/source-control/menu", () =>
        HttpResponse.json({
          data: {
            generation: 0,
            fingerprint: "scope",
            private: false,
            repos: [
              {
                repoId: "code",
                name: "Code",
                base: "base",
                pushed: "head",
                live: null,
                error: null,
                truncated: false,
                commits: [
                  {
                    sha: "1234567",
                    summary: "Real commit",
                    authorName: "Owner",
                    committedAt: 1,
                    parents: ["base"],
                    paths: ["file"],
                  },
                ],
              },
            ],
          },
        }),
      ),
    );
    renderApp(<StoppedSourceControlSidebar {...props} />, {
      route: "/org-g5labs/test-workspace",
    });
    activateSidebarSection("sidebar-source-control-tab");
    expect(await screen.findByLabelText("All changes in Code")).toBeChecked();
    expect(screen.getByLabelText("1234567 in Code")).toBeChecked();
    expect(screen.queryByRole("tab", { name: "History" })).toBeNull();
    expect(screen.queryByRole("combobox")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Open Diff Graph" }),
    ).toBeNull();
  },
);
it("keeps an empty checklist explicit and Show disabled", async () => {
  server.use(
    http.get("/api/v1/workspaces/test-workspace/source-control/menu", () =>
      HttpResponse.json({
        data: {
          generation: 0,
          fingerprint: "empty",
          private: false,
          repos: [],
        },
      }),
    ),
  );
  renderApp(<StoppedSourceControlSidebar {...defaultSidebarProps()} />);
  activateSidebarSection("sidebar-source-control-tab");
  await waitFor(() =>
    expect(screen.queryByText("Loading repository changes…")).toBeNull(),
  );
  expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
});
