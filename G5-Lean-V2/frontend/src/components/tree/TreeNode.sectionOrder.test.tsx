// =============================================================================
// LeftSidebar — section order presets (workbench g5.workbench.phases)
// =============================================================================
// Under the `alignment` layout the sidebar's DEFAULT section order leads with
// project docs; a user's own reordering is preserved whichever preset applies.
// =============================================================================

import { act, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FLOATING_SURFACE_ATTR } from "../../lib/floatingSurface";
import { Modal } from "../ui/Modal";
import { renderApp } from "../../test/render";
import { LeftSidebar } from "./TreeNode";

const WORKSPACE_ID = "test-workspace";
const ORDER_KEY = `g5-sidebar-order-${WORKSPACE_ID}`;

function sidebarProps(overrides: Record<string, unknown> = {}) {
  return {
    isOpen: true,
    width: 280,
    workspaceId: WORKSPACE_ID,
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

/** Rendered section ids (`sidebar-<segment>-tab`) in DOM order. */
function renderedSections(): string[] {
  const sidebar = screen.getByTestId("left-sidebar");
  return Array.from(
    sidebar.querySelectorAll<HTMLElement>('[data-testid^="sidebar-"]'),
  )
    .map((el) => el.dataset.testid ?? "")
    .filter((id) => /^sidebar-[a-z-]+-tab$/.test(id))
    .map((id) => id.replace(/^sidebar-/, "").replace(/-tab$/, ""));
}

/** The expected order restricted to the sections that actually rendered. */
function expectOrder(expected: string[]) {
  const rendered = renderedSections();
  expect(rendered.length).toBeGreaterThan(2);
  expect(rendered).toEqual(expected.filter((id) => rendered.includes(id)));
}

const DEFAULT_ORDER = [
  "ontology",
  "docs",
  "all-docs",
  "source-control",
  "tasks",
  "files",
];
const WORKBENCH_ORDER = [
  "ontology",
  "docs",
  "files",
  "issues",
  "tasks",
  "all-docs",
  "source-control",
];

beforeEach(() => {
  window.localStorage.removeItem(ORDER_KEY);
});

describe("LeftSidebar section order presets", () => {
  it("renders the classic default order without a preset", async () => {
    renderApp(<LeftSidebar {...sidebarProps()} />, {
      route: "/org-g5labs/test-workspace",
    });
    await screen.findByTestId("left-sidebar");
    expectOrder(DEFAULT_ORDER);
    expect(screen.queryByTestId("sidebar-activity-tab")).toBeNull();
    expect(screen.queryByTestId("sidebar-issues-tab")).toBeNull();
    expect(screen.getByRole("button", { name: "Tasks" })).toBeInTheDocument();
  });

  it("leads with Activity under the workbench preset", async () => {
    renderApp(
      <LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expectOrder(WORKBENCH_ORDER);
    expect(screen.getByRole("button", { name: "Tasks" })).toBeInTheDocument();
  });

  it("treats a persisted exact default as uncustomised and follows the preset", async () => {
    window.localStorage.setItem(
      ORDER_KEY,
      JSON.stringify([
        "ontology",
        "project-docs",
        "explainers",
        "all-docs",
        "source-control",
        "tasks",
        "files",
      ]),
    );
    renderApp(
      <LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expectOrder(WORKBENCH_ORDER);
  });

  it("drops the retired Activity section from a persisted order (the Overview control lives in the ribbon)", async () => {
    // An order an earlier default or a drag left with Activity in it.
    window.localStorage.setItem(
      ORDER_KEY,
      JSON.stringify([
        "files",
        "tasks",
        "ontology",
        "project-docs",
        "explainers",
        "all-docs",
        "source-control",
        "issues",
        "activity",
      ]),
    );
    renderApp(
      <LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expectOrder([
      "files",
      "tasks",
      "ontology",
      "docs",
      "all-docs",
      "source-control",
      "issues",
    ]);
    expect(
      JSON.parse(window.localStorage.getItem(ORDER_KEY) ?? "[]"),
    ).not.toContain("activity");
    expect(screen.queryByTestId("sidebar-activity-tab")).toBeNull();
  });

  it("keeps a customised order when the preset switches", async () => {
    // Files first is nobody's default: the user's own arrangement.
    const customised = [
      "files",
      "tasks",
      "ontology",
      "project-docs",
      "explainers",
      "all-docs",
      "source-control",
    ];
    window.localStorage.setItem(ORDER_KEY, JSON.stringify(customised));
    const expected = [
      "files",
      "issues",
      "tasks",
      "ontology",
      "docs",
      "all-docs",
      "source-control",
    ];

    const first = renderApp(
      <LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expectOrder(expected);
    first.unmount();

    renderApp(
      <LeftSidebar {...sidebarProps()} sectionOrderPreset="default" />,
      {
        route: "/org-g5labs/test-workspace",
      },
    );
    await screen.findByTestId("left-sidebar");
    expectOrder(expected);
    // Guided (a later canonical addition) lands at its canonical slot, first,
    // without disturbing the customised remainder; the retired Explainers
    // section is dropped from it (explainers are Project Docs).
    expect(JSON.parse(window.localStorage.getItem(ORDER_KEY) ?? "[]")).toEqual([
      "guided",
      "files",
      "issues",
      ...customised.slice(1).filter((id) => id !== "explainers"),
    ]);
  });
});

it.each(["expanded", "collapse-to-rail", "rail", "expand-on-hover"] as const)(
  "offers its collapse control only in the full sidebar (%s), and no Activity section",
  async (displayMode) => {
    const onCollapse = vi.fn();
    renderApp(
      <LeftSidebar
        {...sidebarProps()}
        sectionOrderPreset="workbench"
        displayMode={displayMode}
        onCollapse={onCollapse}
      />,
    );
    // The Overview control lives in the ribbon (workbench
    // g5.workbench.landing): no Activity section, no Overview row.
    expect(screen.queryByTestId("sidebar-activity-tab")).toBeNull();
    expect(screen.queryByRole("button", { name: "Overview" })).toBeNull();
    if (displayMode === "rail" || displayMode === "expand-on-hover") {
      // The icon rails have no collapse control (owner instruction,
      // 2026-10-29).
      expect(screen.queryByTestId("sidebar-collapse")).toBeNull();
      expect(
        screen.queryByRole("button", { name: "Collapse sidebar" }),
      ).toBeNull();
      return;
    }
    const collapse = screen.getByTestId("sidebar-collapse");
    expect(collapse).toHaveAccessibleName("Collapse sidebar");
    await userEvent.click(collapse);
    expect(onCollapse).toHaveBeenCalledOnce();
  },
);

it("offers no collapse control without a handler", () => {
  renderApp(<LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />);
  expect(screen.getByTestId("left-sidebar")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Collapse sidebar" })).toBeNull();
});

it("migrates the outgoing workbench default", () => {
  localStorage.setItem(
    ORDER_KEY,
    JSON.stringify([
      "project-docs",
      "ontology",
      "files",
      "tasks",
      "explainers",
      "all-docs",
      "source-control",
    ]),
  );
  renderApp(<LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />);
  expect(renderedSections()).toEqual(WORKBENCH_ORDER);
});

it.each([true, false])(
  "offers the Issues import header action only to owners (%s)",
  async (owner) => {
    const onImportTask = vi.fn();
    renderApp(
      <LeftSidebar
        {...sidebarProps()}
        sectionOrderPreset="workbench"
        issuesSection={{
          viewerOwnsWorkspace: owner,
          writeBlockedReason: "Review copy",
          onOpenIssue: vi.fn(),
          onImportTask,
        }}
      />,
    );
    const section = screen.getByTestId("sidebar-issues-tab");
    if (!owner) {
      expect(
        within(section).queryByRole("button", { name: "Import task" }),
      ).toBeNull();
      return;
    }
    const button = within(section).getByRole("button", { name: "Import task" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Review copy");
    await userEvent.click(button);
    expect(onImportTask).not.toHaveBeenCalled();
  },
);

it("offers Terminal on the Files header only when given, disabled with its reason", async () => {
  const onActivate = vi.fn();
  const { rerender } = renderApp(
    <LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />,
  );
  const files = () => screen.getByTestId("sidebar-files-tab");
  expect(
    within(files()).queryByRole("button", { name: "Terminal" }),
  ).toBeNull();
  rerender(
    <LeftSidebar
      {...sidebarProps()}
      sectionOrderPreset="workbench"
      terminalAction={{ onActivate, disabled: true, title: "Start the VM" }}
    />,
  );
  const disabled = within(files()).getByRole("button", { name: "Terminal" });
  expect(disabled).toBeDisabled();
  expect(disabled).toHaveAttribute("title", "Start the VM");
  rerender(
    <LeftSidebar
      {...sidebarProps()}
      sectionOrderPreset="workbench"
      terminalAction={{ onActivate }}
    />,
  );
  await userEvent.click(
    within(files()).getByRole("button", { name: "Terminal" }),
  );
  expect(onActivate).toHaveBeenCalledTimes(1);
});

it("imports from the header without toggling the Issues section", async () => {
  const onImportTask = vi.fn();
  renderApp(
    <LeftSidebar
      {...sidebarProps()}
      sectionOrderPreset="workbench"
      issuesSection={{
        viewerOwnsWorkspace: true,
        onOpenIssue: vi.fn(),
        onImportTask,
      }}
    />,
  );
  await userEvent.click(screen.getByRole("button", { name: "Import task" }));
  expect(onImportTask).toHaveBeenCalledOnce();
  expect(
    screen.getByRole("button", { name: "Issues & tasks" }),
  ).toHaveAttribute("aria-expanded", "false");
});

it.each(["expanded", "collapse-to-rail", "rail", "expand-on-hover"] as const)(
  "keeps Approvals in the ribbon without a second sidebar destination (%s)",
  (displayMode) => {
    renderApp(
      <LeftSidebar
        {...sidebarProps()}
        sectionOrderPreset="workbench"
        displayMode={displayMode}
        onOpenApprovals={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Approvals" })).toBeNull();
  },
);

it("preserves the remaining rail icons with Approvals in the ribbon", () => {
  const view = renderApp(
    <LeftSidebar
      {...sidebarProps()}
      sectionOrderPreset="workbench"
      displayMode="rail"
      onOpenApprovals={vi.fn()}
    />,
  );
  const actions = within(screen.getByTestId("sidebar-rail")).getAllByRole(
    "button",
  );
  expect(actions.map((button) => button.getAttribute("aria-label"))).toEqual([
    "Core Ontology",
    "Project Docs",
    "Files",
    "Issues & tasks",
    "Tasks",
    "Archive",
    "Source Control",
  ]);
  // The dashboard glyph belongs to the ribbon's Overview control now.
  expect(
    screen
      .getByTestId("sidebar-rail")
      .querySelector(".lucide-layout-dashboard"),
  ).toBeNull();
  view.unmount();
  renderApp(
    <LeftSidebar
      {...sidebarProps()}
      sectionOrderPreset="default"
      onOpenApprovals={vi.fn()}
    />,
  );
  expect(screen.queryByRole("button", { name: "Approvals" })).toBeNull();
});

describe("Guided section (ui g5.ui.guided-collapsed)", () => {
  // The key the retired default-open behaviour remembered a collapse under:
  // nothing writes it any more.
  const RETIRED_COLLAPSED_KEY = `g5-sidebar-guided-collapsed-${WORKSPACE_ID}`;

  beforeEach(() => {
    window.localStorage.removeItem(RETIRED_COLLAPSED_KEY);
  });

  it("starts collapsed like every other section under the workbench preset, and is absent under the classic preset", async () => {
    const user = userEvent.setup();
    const first = renderApp(
      <LeftSidebar
        {...sidebarProps()}
        sectionOrderPreset="workbench"
        guidedSection={<div data-testid="guided-body" />}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expect(renderedSections()[0]).toBe("guided");
    expect(screen.queryByTestId("guided-body")).toBeNull();
    const header = () =>
      within(screen.getByTestId("sidebar-guided-tab")).getByRole("button", {
        name: "Guided",
      });
    expect(header()).toHaveAttribute("aria-expanded", "false");
    for (const id of ["docs", "files"]) {
      expect(
        within(screen.getByTestId(`sidebar-${id}-tab`)).getByRole("button", {
          expanded: false,
        }),
      ).toBeInTheDocument();
    }
    // Its header expands and collapses it like the others' do, and nothing
    // is remembered: a remount starts collapsed again.
    await user.click(header());
    expect(await screen.findByTestId("guided-body")).toBeVisible();
    expect(header()).toHaveAttribute("aria-expanded", "true");
    expect(window.localStorage.getItem(RETIRED_COLLAPSED_KEY)).toBeNull();
    await user.click(header());
    expect(screen.queryByTestId("guided-body")).toBeNull();
    expect(window.localStorage.getItem(RETIRED_COLLAPSED_KEY)).toBeNull();
    first.unmount();

    const second = renderApp(
      <LeftSidebar
        {...sidebarProps()}
        sectionOrderPreset="workbench"
        guidedSection={<div data-testid="guided-body" />}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("sidebar-guided-tab");
    expect(screen.queryByTestId("guided-body")).toBeNull();
    second.unmount();

    renderApp(
      <LeftSidebar
        {...sidebarProps()}
        sectionOrderPreset="default"
        guidedSection={<div data-testid="guided-body" />}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expect(screen.queryByTestId("sidebar-guided-tab")).toBeNull();
  });

  it("renders no Guided section without a body, even under the workbench preset", async () => {
    renderApp(
      <LeftSidebar {...sidebarProps()} sectionOrderPreset="workbench" />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("left-sidebar");
    expect(screen.queryByTestId("sidebar-guided-tab")).toBeNull();
  });

  it("expand-on-hover: nothing opens on open; Guided opens from its icon and closes like any section", async () => {
    const user = userEvent.setup();
    renderApp(
      <LeftSidebar
        {...sidebarProps({ displayMode: "expand-on-hover" })}
        sectionOrderPreset="workbench"
        guidedSection={<div data-testid="guided-body" />}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    expect(screen.getByTestId("sidebar-rail-guided")).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    // Hovering its icon opens Guided expanded; leaving closes the overlay.
    await user.hover(screen.getByTestId("sidebar-rail-guided"));
    expect(screen.getByTestId("guided-body")).toBeVisible();
    expect(screen.queryByTestId("sidebar-docs-tab")).toBeNull();
    await user.unhover(screen.getByTestId("left-sidebar"));
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();

    // Its header closes the overlay like every other section's header.
    await user.hover(screen.getByTestId("sidebar-rail-guided"));
    await user.click(
      within(screen.getByTestId("sidebar-guided-tab")).getByRole("button", {
        name: "Guided",
      }),
    );
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    expect(window.localStorage.getItem(RETIRED_COLLAPSED_KEY)).toBeNull();
  });

  it("rail: nothing opens on open; icon activation opens and closes Guided like any section", async () => {
    const user = userEvent.setup();
    renderApp(
      <LeftSidebar
        {...sidebarProps({ displayMode: "rail" })}
        sectionOrderPreset="workbench"
        guidedSection={<div data-testid="guided-body" />}
      />,
      { route: "/org-g5labs/test-workspace" },
    );
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    await user.click(screen.getByTestId("sidebar-rail-guided"));
    expect(await screen.findByTestId("guided-body")).toBeVisible();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    await user.click(screen.getByTestId("sidebar-rail-guided"));
    expect(await screen.findByTestId("guided-body")).toBeVisible();
    await user.click(
      within(screen.getByTestId("sidebar-guided-tab")).getByRole("button", {
        name: "Guided",
      }),
    );
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
  });

  it("drops a hidden sticky Guided on a switch to Base and opens nothing on the way back", async () => {
    const user = userEvent.setup();
    let switchTo: ((workspaceId: string) => void) | null = null;
    function Switching() {
      const [workspaceId, setWorkspaceId] = useState(WORKSPACE_ID);
      useEffect(() => {
        switchTo = setWorkspaceId;
      }, []);
      return (
        <LeftSidebar
          {...sidebarProps({ displayMode: "expand-on-hover", workspaceId })}
          sectionOrderPreset="workbench"
          // Base has no Guided section.
          guidedSection={
            workspaceId === "base" ? undefined : (
              <div data-testid="guided-body" />
            )
          }
        />
      );
    }
    renderApp(<Switching />, { route: "/org-g5labs/test-workspace" });
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    await user.hover(screen.getByTestId("sidebar-rail-guided"));
    expect(screen.getByTestId("guided-body")).toBeVisible();

    // Switching to Base with Guided open: Guided is hidden there, so the
    // overlay closes and a hover opens the first visible section rather
    // than an empty Guided panel.
    act(() => switchTo?.("base"));
    await screen.findByTestId("sidebar-rail");
    expect(screen.queryByTestId("sidebar-rail-guided")).toBeNull();
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    await user.hover(screen.getByTestId("left-sidebar"));
    expect(screen.getByTestId("sidebar-rail-overlay")).toBeInTheDocument();
    expect(screen.queryByTestId("sidebar-guided-tab")).toBeNull();
    // The first visible section opens, Base included.
    expect(screen.getByTestId("sidebar-ontology-tab")).toBeInTheDocument();
    await user.unhover(screen.getByTestId("left-sidebar"));
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();

    // Back to the workspace: Guided is listed again and nothing opens.
    act(() => switchTo?.(WORKSPACE_ID));
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
  });

  it("keeps the overlay while a section's portaled popover is clicked or hovered", async () => {
    const user = userEvent.setup();
    const onAction = vi.fn();
    // Stand-in for the roster popover: portaled to the body with the
    // floating-surface marker (lib/floatingSurface.ts).
    const popover = createPortal(
      <div {...{ [FLOATING_SURFACE_ATTR]: "" }} data-testid="floating-popover">
        <h4 data-testid="popover-text">Still missing</h4>
        <button type="button" onClick={onAction}>
          View approvals
        </button>
      </div>,
      document.body,
    );
    renderApp(
      <>
        <LeftSidebar
          {...sidebarProps({ displayMode: "expand-on-hover" })}
          sectionOrderPreset="workbench"
          guidedSection={
            <div data-testid="guided-body">
              <span data-testid="anchor" />
              {popover}
            </div>
          }
        />
        <div data-testid="elsewhere" />
      </>,
      { route: "/org-g5labs/test-workspace" },
    );
    // Open Guided from its icon (a direct event, so user-event's later
    // pointer moves start from no previous position).
    fireEvent.mouseEnter(await screen.findByTestId("sidebar-rail-guided"));
    expect(screen.getByTestId("guided-body")).toBeVisible();
    // Hover the overlay, then leave it into the popover: it stays.
    await user.hover(screen.getByTestId("anchor"));
    fireEvent.mouseLeave(screen.getByTestId("left-sidebar"), {
      relatedTarget: screen.getByTestId("floating-popover"),
    });
    expect(screen.getByTestId("guided-body")).toBeVisible();
    // A click in the popover reaches its button with the overlay still up
    // (explicit events: user-event's pointer move reports no related target,
    // unlike a browser moving into the portaled popover).
    const viewApprovals = screen.getByRole("button", {
      name: "View approvals",
    });
    fireEvent.pointerDown(viewApprovals);
    fireEvent.mouseDown(viewApprovals);
    fireEvent.click(viewApprovals);
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("guided-body")).toBeVisible();
    // Clicking the popover's non-focusable text blurs the focused trigger
    // in the overlay with no related target: still inside.
    const header = within(screen.getByTestId("sidebar-guided-tab")).getByRole(
      "button",
      { name: "Guided" },
    );
    header.focus();
    fireEvent.pointerDown(screen.getByTestId("popover-text"));
    fireEvent.focusOut(header, { relatedTarget: null });
    expect(screen.getByTestId("guided-body")).toBeVisible();
    // A click elsewhere still dismisses.
    await user.click(screen.getByTestId("elsewhere"));
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
  });

  it("keeps the overlay while a section's dialog (a preview control's) is opened and used, and leaves Escape to it", async () => {
    const user = userEvent.setup();
    // Stand-in for a preview control: a button in the section opens a real
    // Modal, portaled to the body.
    function PreviewStandIn({ onAction }: { onAction: () => void }) {
      const [open, setOpen] = useState(false);
      return (
        <div data-testid="guided-body">
          <button type="button" onClick={() => setOpen(true)}>
            Preview
          </button>
          {open && (
            <Modal onClose={() => setOpen(false)} ariaLabel="Workspace preview">
              <button type="button" onClick={onAction}>
                Start preview
              </button>
            </Modal>
          )}
        </div>
      );
    }
    for (const displayMode of ["rail", "expand-on-hover"] as const) {
      const onAction = vi.fn();
      const rendered = renderApp(
        <LeftSidebar
          {...sidebarProps({ displayMode })}
          sectionOrderPreset="workbench"
          guidedSection={<PreviewStandIn onAction={onAction} />}
        />,
        { route: "/org-g5labs/test-workspace" },
      );
      // Open Guided from its icon (direct events, so user-event's later
      // pointer moves start from no previous position).
      const icon = await screen.findByTestId("sidebar-rail-guided");
      if (displayMode === "rail") fireEvent.click(icon);
      else fireEvent.mouseEnter(icon);
      expect(await screen.findByTestId("guided-body")).toBeVisible();
      await user.hover(screen.getByTestId("guided-body"));
      // A direct click: user-event's pointer move between two elements of the
      // sidebar reports no related target, which a browser never does.
      fireEvent.click(screen.getByRole("button", { name: "Preview" }));
      const dialog = screen.getByRole("dialog", { name: "Workspace preview" });
      // The pointer and focus move into the dialog: the overlay stays.
      fireEvent.mouseLeave(screen.getByTestId("left-sidebar"), {
        relatedTarget: dialog,
      });
      fireEvent.focusOut(screen.getByRole("button", { name: "Preview" }), {
        relatedTarget: dialog,
      });
      const start = screen.getByRole("button", { name: "Start preview" });
      fireEvent.pointerDown(start);
      fireEvent.mouseDown(start);
      fireEvent.click(start);
      expect(onAction).toHaveBeenCalledTimes(1);
      expect(screen.getByTestId("guided-body")).toBeVisible();
      // Escape closes the dialog, not the overlay under it.
      await user.keyboard("{Escape}");
      expect(
        screen.queryByRole("dialog", { name: "Workspace preview" }),
      ).toBeNull();
      expect(screen.getByTestId("guided-body")).toBeVisible();
      rendered.unmount();
    }
  });

  it("opens nothing when Guided arrives after mount, hovered or not", async () => {
    const user = userEvent.setup();
    // Guided arrives later (its facts load after the sidebar mounts).
    let arrive: (() => void) | null = null;
    function LateGuided() {
      const [arrived, setArrived] = useState(false);
      useEffect(() => {
        arrive = () => setArrived(true);
      }, []);
      return (
        <LeftSidebar
          {...sidebarProps({ displayMode: "expand-on-hover" })}
          sectionOrderPreset="workbench"
          guidedSection={
            arrived ? <div data-testid="guided-body" /> : undefined
          }
        />
      );
    }

    const first = renderApp(<LateGuided />, {
      route: "/org-g5labs/test-workspace",
    });
    await screen.findByTestId("sidebar-rail");
    act(() => arrive?.());
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.queryByTestId("sidebar-rail-overlay")).toBeNull();
    first.unmount();

    // Already hovering another section: the overlay stays where it is.
    arrive = null;
    renderApp(<LateGuided />, { route: "/org-g5labs/test-workspace" });
    await user.hover(screen.getByTestId("sidebar-rail-docs"));
    expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
    act(() => arrive?.());
    await screen.findByTestId("sidebar-rail-guided");
    expect(screen.getByTestId("sidebar-docs-tab")).toBeInTheDocument();
    expect(screen.queryByTestId("guided-body")).toBeNull();
  });
});
