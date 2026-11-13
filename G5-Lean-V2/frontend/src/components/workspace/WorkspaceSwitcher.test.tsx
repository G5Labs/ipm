import { describe, expect, it, beforeEach, vi } from "vitest";
import { screen, within, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { renderApp } from "../../test/render";
import { createMockUser, createMockWorkspace } from "../../test/fixtures";
import { MINIMAL_WORKSPACE_UI_CAPABILITIES } from "../../lib/capabilities";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

const useWorkspacesMock = vi.fn();

vi.mock("../../hooks/useWorkspace", async () => {
  const actual = await vi.importActual<
    typeof import("../../hooks/useWorkspace")
  >("../../hooks/useWorkspace");
  return {
    ...actual,
    useWorkspaces: (...args: unknown[]) => useWorkspacesMock(...args),
  };
});

describe("WorkspaceSwitcher", () => {
  beforeEach(() => {
    useWorkspacesMock.mockReset();
  });

  it("shows a directory icon for an explicit empty label and bounds its dropdown", async () => {
    const user = userEvent.setup();
    useWorkspacesMock.mockReturnValue({ status: "ok", data: [] });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={null}
        emptyLabel="Workspaces"
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    const trigger = screen.getByRole("button", { name: /workspaces/i });
    expect(
      within(trigger).getByTestId("workspace-directory-icon"),
    ).toBeVisible();
    await user.click(trigger);
    expect(screen.getByRole("listbox")).toHaveClass(
      "w-[min(420px,calc(100vw-1.5rem))]",
    );
  });

  it("shows a fork button for the current workspace and opens the fork flow", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Current Project",
    });
    const onForkCurrentWorkspace = vi.fn();
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
        onForkCurrentWorkspace={onForkCurrentWorkspace}
      />,
      { user: createMockUser() },
    );

    const forkButton = screen.getByRole("button", {
      name: /fork current project/i,
    });

    expect(forkButton).toBeVisible();
    expect(
      screen.queryByTestId("workspace-directory-icon"),
    ).not.toBeInTheDocument();

    await user.click(forkButton);

    expect(onForkCurrentWorkspace).toHaveBeenCalledTimes(1);
  });

  it("shows a Read-only badge on the trigger and dropdown row for a read-only workspace (M25)", async () => {
    const readOnlyWorkspace = createMockWorkspace({
      id: "ws-ro",
      name: "Locked Project",
      readOnly: true,
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [readOnlyWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={readOnlyWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    // Trigger badge next to the workspace name.
    expect(screen.getAllByTestId("workspace-readonly-badge").length).toBe(1);

    // Opening the dropdown surfaces the compact badge on the matching row.
    await user.click(screen.getByRole("button", { name: /locked project/i }));
    expect(
      screen.getAllByTestId("workspace-readonly-badge").length,
    ).toBeGreaterThanOrEqual(2);
  });

  it("keeps review copies out of the tree", async () => {
    const source = createMockWorkspace({ id: "ws-src", name: "kaihui-test3" });
    const reviewCopy = createMockWorkspace({
      id: "ws-review",
      name: "kaihui-test3-review-alex-chen",
      clonedFromId: "ws-src",
      isReview: true,
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [source, reviewCopy],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={source}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    // The source is not a review workspace: no tag on the trigger.
    expect(
      screen.queryByTestId("workspace-review-badge"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /kaihui-test3/i }));
    // Expand the source so its review copy row is visible.
    const sourceRow = screen
      .getAllByTestId("workspace-switcher-row")
      .find((row) => row.textContent?.includes("kaihui-test3"));
    expect(sourceRow).toBeDefined();
    const toggle = within(sourceRow as HTMLElement).queryByRole("button", {
      name: /expand/i,
    });
    if (toggle) await user.click(toggle);

    // The copy is not a row of the tree: it is reached from the source's
    // Approvals page.
    expect(
      screen
        .getAllByTestId("workspace-switcher-row")
        .find((row) => row.textContent?.includes("review-alex-chen")),
    ).toBeUndefined();
  });

  it("lists the open review copy as its own row under its source (g5.workspace.ui.review-copy-row)", async () => {
    const source = createMockWorkspace({ id: "ws-src", name: "kaihui-test3" });
    const reviewCopy = createMockWorkspace({
      id: "ws-review",
      name: "kaihui-test3-review-alex-chen",
      clonedFromId: "ws-src",
      isReview: true,
    });
    const otherCopy = createMockWorkspace({
      id: "ws-review-2",
      name: "kaihui-test3-review-sam",
      clonedFromId: "ws-src",
      isReview: true,
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [source, reviewCopy, otherCopy],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={reviewCopy}
        currentWorkspaceIsReview
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
        onForkWorkspace={vi.fn()}
        onRemoveWorkspace={vi.fn()}
        canRemoveWorkspace={() => true}
        capabilities={{
          ...MINIMAL_WORKSPACE_UI_CAPABILITIES,
          cloudWorkspaceWorkflows: true,
        }}
      />,
      { user: createMockUser() },
    );

    await user.click(
      screen.getByRole("button", { name: /kaihui-test3-review-alex-chen/i }),
    );
    // The copy's ancestors open by themselves, so its row is visible at once.
    const rows = screen.getAllByTestId("workspace-switcher-row");
    const copyRow = rows.find((row) =>
      row.textContent?.includes("review-alex-chen"),
    );
    expect(copyRow).toBeDefined();
    // The row keeps Remove but never offers Fork: the server refuses forking
    // a copy (`g5.governance.review-workspace.exclusions`).
    expect(
      within(copyRow as HTMLElement).getByRole("button", { name: /remove/i }),
    ).toBeInTheDocument();
    expect(
      within(copyRow as HTMLElement).queryByRole("button", { name: /fork/i }),
    ).toBeNull();
    // Other reviewers' copies stay out of the tree.
    expect(
      rows.find((row) => row.textContent?.includes("review-sam")),
    ).toBeUndefined();
  });

  it("renders a per-row fork button that forks that specific workspace", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const siblingWorkspace = createMockWorkspace({
      id: "ws-sibling",
      name: "Sibling",
    });
    const onForkWorkspace = vi.fn();
    const onSelectWorkspace = vi.fn();
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, siblingWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={onSelectWorkspace}
        onNewWorkspace={vi.fn()}
        onForkWorkspace={onForkWorkspace}
      />,
      { user: createMockUser() },
    );

    // Open the dropdown (only the trigger button exists while closed).
    await user.click(screen.getByRole("button", { name: /mainline/i }));

    // The sibling row exposes its own fork button; clicking it forks that
    // specific workspace and must not also select/navigate to the row.
    const siblingOption = await screen.findByRole("option", {
      name: /sibling/i,
    });
    await user.click(
      within(siblingOption).getByRole("button", { name: /fork sibling/i }),
    );

    expect(onForkWorkspace).toHaveBeenCalledTimes(1);
    expect(onForkWorkspace).toHaveBeenCalledWith(
      expect.objectContaining({ id: "ws-sibling" }),
    );
    expect(onSelectWorkspace).not.toHaveBeenCalled();
  });

  it("renders a per-row remove button for eligible non-Base workspaces", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const removableWorkspace = createMockWorkspace({
      id: "ws-removable",
      name: "Scratch Branch",
      isBase: false,
    });
    const onRemoveWorkspace = vi.fn();
    const onSelectWorkspace = vi.fn();
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, removableWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={onSelectWorkspace}
        onNewWorkspace={vi.fn()}
        onRemoveWorkspace={onRemoveWorkspace}
        canRemoveWorkspace={() => true}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    const removableOption = await screen.findByRole("option", {
      name: /scratch branch/i,
    });
    await user.click(
      within(removableOption).getByRole("button", {
        name: /remove scratch branch/i,
      }),
    );

    expect(onRemoveWorkspace).toHaveBeenCalledTimes(1);
    expect(onRemoveWorkspace).toHaveBeenCalledWith(
      expect.objectContaining({ id: "ws-removable" }),
    );
    expect(onSelectWorkspace).not.toHaveBeenCalled();
  });

  it("keeps per-row remove visible when cloud workflows are disabled", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
      isBase: false,
    });
    const onRemoveWorkspace = vi.fn();
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
        onRemoveWorkspace={onRemoveWorkspace}
        canRemoveWorkspace={() => true}
        capabilities={MINIMAL_WORKSPACE_UI_CAPABILITIES}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    const currentOption = await screen.findByRole("option", {
      name: /mainline/i,
    });
    await user.click(
      within(currentOption).getByRole("button", {
        name: /remove mainline/i,
      }),
    );

    expect(onRemoveWorkspace).toHaveBeenCalledTimes(1);
  });

  it("does not render remove for Base workspaces", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const baseWorkspace = createMockWorkspace({
      id: "ws-base",
      name: "Base Workspace",
      isBase: true,
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, baseWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
        onRemoveWorkspace={vi.fn()}
        canRemoveWorkspace={() => true}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    const baseOption = await screen.findByRole("option", {
      name: /base workspace/i,
    });

    expect(
      within(baseOption).queryByRole("button", {
        name: /remove base workspace/i,
      }),
    ).not.toBeInTheDocument();
  });

  it("does not render remove when the caller says the row is ineligible", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const ineligibleWorkspace = createMockWorkspace({
      id: "ws-ineligible",
      name: "Teammate Draft",
      isBase: false,
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, ineligibleWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
        onRemoveWorkspace={vi.fn()}
        canRemoveWorkspace={() => false}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    const ineligibleOption = await screen.findByRole("option", {
      name: /teammate draft/i,
    });

    expect(
      within(ineligibleOption).queryByRole("button", {
        name: /remove teammate draft/i,
      }),
    ).not.toBeInTheDocument();
  });

  it("activates the row fork button via keyboard (Enter) without selecting the row", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const siblingWorkspace = createMockWorkspace({
      id: "ws-sibling",
      name: "Sibling",
    });
    const onForkWorkspace = vi.fn();
    const onSelectWorkspace = vi.fn();
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, siblingWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={onSelectWorkspace}
        onNewWorkspace={vi.fn()}
        onForkWorkspace={onForkWorkspace}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    // Focus the sibling row's fork button and activate it with Enter. The
    // container's listbox key handler must not swallow Enter meant for the
    // button (otherwise keyboard users can't fork).
    const siblingOption = await screen.findByRole("option", {
      name: /sibling/i,
    });
    const forkButton = within(siblingOption).getByRole("button", {
      name: /fork sibling/i,
    });
    forkButton.focus();
    await user.keyboard("{Enter}");

    expect(onForkWorkspace).toHaveBeenCalledTimes(1);
    expect(onForkWorkspace).toHaveBeenCalledWith(
      expect.objectContaining({ id: "ws-sibling" }),
    );
    expect(onSelectWorkspace).not.toHaveBeenCalled();
  });

  it("hides repo/branch text and marks leaf rows with a dot", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Solo Project",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /solo project/i }));

    const option = await screen.findByRole("option", {
      name: /solo project/i,
    });
    // Repo count + branch names are no longer rendered.
    expect(within(option).queryByText(/repo/i)).not.toBeInTheDocument();
    expect(within(option).queryByText(/main/i)).not.toBeInTheDocument();
    // A leaf workspace shows a dot marker instead of an expand/collapse control.
    expect(
      within(option).getByTestId("workspace-leaf-marker"),
    ).toBeInTheDocument();
    expect(option.querySelector(".lucide-plus")).toBeNull();
    expect(option.querySelector(".lucide-minus")).toBeNull();
  });

  it("injects the current workspace when it is missing from the first page", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Production Project",
    });
    const otherWorkspace = createMockWorkspace({
      id: "ws-other",
      name: "Recent Project",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [otherWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(
      screen.getByRole("button", { name: /production project/i }),
    );

    const currentOption = screen.getByRole("option", {
      name: /production project/i,
    });
    expect(currentOption).toHaveAttribute("aria-selected", "true");
  });

  it("collapses cloned-workspace subtrees by default and toggles on click", async () => {
    const parentWorkspace = createMockWorkspace({
      id: "ws-parent",
      name: "Platform Base",
      clonedFromId: null,
    });
    const childWorkspace = createMockWorkspace({
      id: "ws-child",
      name: "Auth Refactor",
      clonedFromId: "ws-parent",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [parentWorkspace, childWorkspace],
    });

    const { container } = renderApp(
      <WorkspaceSwitcher
        currentWorkspace={parentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /platform base/i }));

    const parentOption = await screen.findByRole("option", {
      name: /platform base/i,
    });
    // Collapsed by default: child hidden, parent shows a "+" (no folder icons).
    expect(
      screen.queryByRole("option", { name: /auth refactor/i }),
    ).not.toBeInTheDocument();
    expect(parentOption.querySelector(".lucide-plus")).toBeTruthy();
    expect(parentOption.querySelector(".lucide-minus")).toBeNull();
    expect(container.querySelector(".lucide-folder")).toBeNull();
    expect(container.querySelector(".lucide-folder-open")).toBeNull();
    expect(
      screen.getByRole("button", { name: /new workspace/i }),
    ).toBeVisible();
    // A parent uses an expand/collapse control, never a leaf dot.
    expect(
      within(parentOption).queryByTestId("workspace-leaf-marker"),
    ).not.toBeInTheDocument();

    // Expanding reveals the leaf child, which shows a dot marker.
    await user.click(
      screen.getByRole("button", { name: /expand platform base/i }),
    );
    const childOption = screen.getByRole("option", { name: /auth refactor/i });
    expect(childOption).toBeVisible();
    expect(parentOption.querySelector(".lucide-minus")).toBeTruthy();
    expect(
      within(childOption).getByTestId("workspace-leaf-marker"),
    ).toBeInTheDocument();

    // Collapsing hides it again.
    await user.click(
      screen.getByRole("button", { name: /collapse platform base/i }),
    );
    expect(
      screen.queryByRole("option", { name: /auth refactor/i }),
    ).not.toBeInTheDocument();
    expect(parentOption.querySelector(".lucide-plus")).toBeTruthy();
  });

  it("does not force-expand the active workspace's parent", async () => {
    const parentWorkspace = createMockWorkspace({
      id: "ws-parent",
      name: "Demo",
      clonedFromId: null,
    });
    const childWorkspace = createMockWorkspace({
      id: "ws-child",
      name: "Demo Copy",
      clonedFromId: "ws-parent",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [parentWorkspace, childWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={childWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /demo copy/i }));

    // The active workspace is the nested child, but its parent stays collapsed
    // by default — it is not force-expanded, so the child is not rendered.
    const parentOption = await screen.findByRole("option", { name: /demo/i });
    expect(parentOption.querySelector(".lucide-plus")).toBeTruthy();
    expect(
      screen.queryByRole("option", { name: /demo copy/i }),
    ).not.toBeInTheDocument();
  });

  it("auto-expands parent folders for search matches", async () => {
    const parentWorkspace = createMockWorkspace({
      id: "ws-parent",
      name: "Platform Base",
      clonedFromId: null,
    });
    const childWorkspace = createMockWorkspace({
      id: "ws-child",
      name: "Search Matched Child",
      clonedFromId: "ws-parent",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [parentWorkspace, childWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={parentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /platform base/i }));

    // Collapsed by default — the child is hidden until search reveals it.
    expect(
      screen.queryByRole("option", { name: /search matched child/i }),
    ).not.toBeInTheDocument();

    await user.type(
      screen.getByPlaceholderText(/search workspaces/i),
      "matched",
    );

    expect(
      screen.getByRole("option", { name: /search matched child/i }),
    ).toBeVisible();
  });

  it("hides project creation and fork actions when cloud workflows are disabled", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Current Project",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
        onForkCurrentWorkspace={vi.fn()}
        onForkWorkspace={vi.fn()}
        capabilities={MINIMAL_WORKSPACE_UI_CAPABILITIES}
      />,
      { user: createMockUser() },
    );

    expect(
      screen.queryByRole("button", { name: /fork current project/i }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /current project/i }));

    expect(
      screen.queryByRole("button", { name: /new workspace/i }),
    ).not.toBeInTheDocument();

    // Per-row fork buttons are gated too; retired workspace status is absent.
    expect(
      screen.queryByRole("button", { name: /^fork /i }),
    ).not.toBeInTheDocument();
    const currentOption = screen.getByRole("option", {
      name: /current project/i,
    });
    expect(within(currentOption).queryByText("active")).not.toBeInTheDocument();
  });

  it("renders each row name as an anchor to the workspace URL with a full-name tooltip", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const siblingWorkspace = createMockWorkspace({
      id: "ws-sibling",
      name: "PairSoftPlatform-Demo-Button-Red-Sync",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, siblingWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="pairsoft-davide"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    // The row name is a real anchor whose href is the workspace URL, so the
    // browser status bar reveals the full (often truncated) name on hover, and
    // the title attribute surfaces it as a tooltip.
    const link = await screen.findByRole("link", {
      name: "PairSoftPlatform-Demo-Button-Red-Sync",
    });
    expect(link).toHaveAttribute(
      "href",
      "/pairsoft-davide/PairSoftPlatform-Demo-Button-Red-Sync",
    );
    expect(link).toHaveAttribute(
      "title",
      "PairSoftPlatform-Demo-Button-Red-Sync",
    );
  });

  it("URL-encodes the workspace name in the row anchor href", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const siblingWorkspace = createMockWorkspace({
      id: "ws-sibling",
      name: "My Repo #2",
    });
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, siblingWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={vi.fn()}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));

    // Spaces and reserved characters in the name must be percent-encoded so
    // the hover URL / open-in-new-tab affordance produces a valid link.
    const link = await screen.findByRole("link", { name: "My Repo #2" });
    expect(link).toHaveAttribute("href", "/g5labs/My%20Repo%20%232");
  });

  it("selects via onSelectWorkspace on plain anchor click but defers to the browser on modifier-click", async () => {
    const currentWorkspace = createMockWorkspace({
      id: "ws-current",
      name: "Mainline",
    });
    const siblingWorkspace = createMockWorkspace({
      id: "ws-sibling",
      name: "Sibling",
    });
    const onSelectWorkspace = vi.fn();
    const user = userEvent.setup();

    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [currentWorkspace, siblingWorkspace],
    });

    renderApp(
      <WorkspaceSwitcher
        currentWorkspace={currentWorkspace}
        orgId="org-g5labs"
        orgSlug="g5labs"
        onSelectWorkspace={onSelectWorkspace}
        onNewWorkspace={vi.fn()}
      />,
      { user: createMockUser() },
    );

    await user.click(screen.getByRole("button", { name: /mainline/i }));
    const link = await screen.findByRole("link", { name: "Sibling" });

    // Modifier-click keeps the browser default (open in a new tab): the SPA
    // selection handler must NOT fire.
    // Prevent jsdom's unsupported navigation after the component has had the
    // chance to preserve the modifier-click browser path.
    link.addEventListener("click", (event) => event.preventDefault(), {
      once: true,
    });
    await user.keyboard("{Meta>}");
    await user.click(link);
    await user.keyboard("{/Meta}");
    expect(onSelectWorkspace).not.toHaveBeenCalled();

    // Plain click selects the row via the SPA handler.
    await user.click(link);
    expect(onSelectWorkspace).toHaveBeenCalledTimes(1);
    expect(onSelectWorkspace).toHaveBeenCalledWith("ws-sibling");
  });
});
