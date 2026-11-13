import { test, expect } from "@playwright/test";
import {
  activateSidebarSection,
  createWorkspaceClone,
  deleteWorkspaceById,
  showWorkspaceTabs,
  waitForWorkspaceLoad,
  TEST_ORG,
  TEST_WORKSPACE,
  type WorkspaceClone,
} from "../helpers";

test.describe("Workspace Navigation", () => {
  let isolatedWorkspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    isolatedWorkspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: `workspace-nav-${testInfo.title}`,
    });
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, isolatedWorkspace?.id).catch(() => {});
  });

  test("workspace page loads with all panels", async ({ page }) => {
    // Verify all major panels are present
    await expect(page.getByTestId("left-sidebar")).toBeVisible();
    await expect(page.getByTestId("center-panel")).toBeVisible();
    // Right panel or its toggle should be in the DOM
    const rightPanel = page.getByTestId("right-panel");
    const toggleBtn = page.getByTestId("toggle-right-panel");
    const rpVisible = await rightPanel.isVisible().catch(() => false);
    const tbVisible = await toggleBtn.isVisible().catch(() => false);
    expect(rpVisible || tbVisible).toBeTruthy();
    // A clean workspace shows the empty Task Queue. The Task Queue is a rail
    // section, so activate it before checking its empty state.
    await activateSidebarSection(page, "sidebar-tasks-tab");
    await expect(page.getByText("No tasks yet.").first()).toBeVisible();
  });

  test("offers Files and Core Ontology navigation", async ({ page }) => {
    const sidebar = page.getByTestId("left-sidebar");
    await expect(sidebar).toBeVisible();
    await expect(sidebar.getByTestId("sidebar-files-tab")).toBeVisible();
    await expect(sidebar.getByTestId("sidebar-ontology-tab")).toBeVisible();
    await activateSidebarSection(page, "sidebar-ontology-tab");
    await expect(page.getByTestId("ontology-tree")).toBeVisible();
  });

  test("sidebar is resizable", async ({ page }) => {
    const sidebar = page.getByTestId("left-sidebar");
    const initialWidth = await sidebar.evaluate(
      (el) => el.getBoundingClientRect().width,
    );

    // Find and drag the resize handle
    const handle = page.getByTestId("sidebar-resize-handle").first();
    if (await handle.isVisible()) {
      const box = await handle.boundingBox();
      if (box) {
        await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
        await page.mouse.down();
        await page.mouse.move(box.x + 100, box.y + box.height / 2);
        await page.mouse.up();

        const newWidth = await sidebar.evaluate(
          (el) => el.getBoundingClientRect().width,
        );
        expect(newWidth).toBeGreaterThan(initialWidth);
      }
    }
  });

  test("opens the workspace details page from the top-bar info button", async ({
    page,
  }) => {
    // The Info entrypoint lives in the WorkspaceSwitcher trigger row, between
    // the switcher trigger and the fork button.
    const infoButton = page.getByTestId("workspace-info-button");
    await expect(infoButton).toBeVisible();
    await infoButton.click();

    // The read-only Workspace Details panel opens as a center tab and the URL
    // reflects the shareable query-param view.
    const panel = page.getByTestId("workspace-details-panel");
    await expect(panel).toBeVisible();
    await expect(page).toHaveURL(/view=details/);

    // Displays the current workspace name and the read-only transition control.
    // The panel also repeats the name inside each member's clone command, so
    // anchor on the heading rather than any text match.
    await expect(
      panel.getByRole("heading", { name: isolatedWorkspace.name }),
    ).toBeVisible();
    await expect(page.getByTestId("workspace-readonly-toggle")).toBeVisible();

    // Deep-linking directly to ?view=details re-opens the panel on load.
    await page.goto(`${isolatedWorkspace.path}?view=details`, {
      waitUntil: "domcontentloaded",
    });
    await waitForWorkspaceLoad(page);
    await expect(page.getByTestId("workspace-details-panel")).toBeVisible();
  });

  // Center surfaces that had no URL form (issue #561, g5.workbench.tabs): a
  // link lands on them and the address bar names them.
  test("deep links land on Pull Requests and a guide doc", async ({ page }) => {
    // The center tab bar is hidden by default; the selected tab is the
    // Pull Requests surface's stable locator.
    await showWorkspaceTabs(page);
    await page.goto(`${isolatedWorkspace.path}?view=pull-requests`, {
      waitUntil: "domcontentloaded",
    });
    await waitForWorkspaceLoad(page);
    await expect(
      page.getByRole("tab", { name: /Pull Requests/ }),
    ).toHaveAttribute("aria-selected", "true");
    await expect(page).toHaveURL(/view=pull-requests/);

    await page.goto(`${isolatedWorkspace.path}/guide/getting-started`, {
      waitUntil: "domcontentloaded",
    });
    await waitForWorkspaceLoad(page);
    await expect(page.getByTestId("guide-doc-view")).toBeVisible();
    await expect(page).toHaveURL(/\/guide\/getting-started$/);
  });
});
