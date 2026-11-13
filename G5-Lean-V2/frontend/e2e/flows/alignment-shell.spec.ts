import { test, expect, type Page, type Response } from "@playwright/test";
import { createReviewActor } from "../workspace-review";
import {
  GUIDED_JOURNEY_ENABLED,
  authedApi,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  docIdForPath,
  expectSelectedAgentProvider,
  openWorkspacePath,
  readActiveSubject,
  readCurrentOrgContext,
  releaseChatTurn,
  resolveWorkspaceId,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  waitForWorkspaceLoad,
  waitForWorkspaceVmStatus,
  workspaceCloneName,
  TEST_ORG,
  TEST_WORKSPACE,
  type ChatTurnIdentity,
  type WorkspaceClone,
  showWorkspaceTabs,
  openWorkspaceSettingsPage,
} from "../helpers";

/** The timeline's filter groups in row order (`lib/activityFilters.ts`). */
const ACTIVITY_FILTER_GROUP_IDS = [
  "issue_discussion",
  "conversations",
  "tasks",
  "commits",
  "approvals",
  "pull_requests",
  "guided_steps",
] as const;

// The workbench shell (ui g5.ui.workbench-shell, g5.workbench.phases.state).
// Playwright runs outside Vite/React, so the Guided gate comes from the
// shared helpers, mirrored from the Node-side env the stack was started with.

test.describe("Alignment workbench shell", () => {
  let isolatedWorkspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    await showWorkspaceTabs(page);
    // The clone is owned by the E2E user, so the chevrons are interactive.
    isolatedWorkspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: `alignment-${testInfo.title}`,
    });
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, isolatedWorkspace?.id).catch(() => {});
  });

  test("the workbench shows the chevrons and a phase move survives reload", async ({
    page,
  }) => {
    // Settings offers no Workspace panels select: the workbench is the one
    // shell (ui g5.ui.workbench-shell).
    const settingsPage = await openWorkspaceSettingsPage(page);
    await expect(
      settingsPage.getByTestId("phase-bar-style-select"),
    ).toBeVisible();
    await expect(
      settingsPage.getByTestId("workspace-layout-select"),
    ).toHaveCount(0);

    // Back on the workspace: chevrons + ribbon head the page
    // (g5.workbench.phases).
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    const chevrons = page.getByTestId("phase-chevrons");
    await expect(chevrons).toBeVisible({ timeout: 15_000 });
    await expect(chevrons).toHaveAttribute("data-phase", "be");
    await expect(page.getByTestId("action-ribbon")).toBeVisible();
    // Default header styles (g5.workbench.header-styles): minimal chevrons,
    // the dynamic action bar, no branch chip.
    await expect(chevrons).toHaveAttribute("data-variant", "minimal");
    await expect(page.getByTestId("action-ribbon")).toHaveAttribute(
      "data-variant",
      "dynamic",
    );
    await expect(page.getByTestId("ribbon-branch")).toHaveCount(0);

    // The workspace opens on its landing above Activity
    // (g5.workbench.landing); the phase lives only in the chevrons. No
    // requirement is attached to this clone's target, so the chevrons carry
    // no approval meter (g5.workbench.gates).
    const header = page.getByTestId("workspace-landing");
    await expect(header).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("workspace-activity-panel")).toBeVisible();
    await expect(chevrons).toHaveAttribute("data-phase", "be");
    await expect(header).not.toContainText("Business Engineering");
    // A fresh fork has no PR: formal approval state starts at submission.
    await expect(page.getByTestId("landing-implementation-empty")).toHaveText(
      "No implementation description yet.",
    );
    await expect(page.getByTestId("landing-summary-approvals")).toContainText(
      "No approvals required",
    );
    await expect(page.getByTestId("phase-approval-meter-be")).toHaveCount(0);
    await expect(page.getByTestId("phase-approval-meter-ie")).toHaveCount(0);
    await expect(page.getByTestId("center-content-tab").first()).toHaveText(
      /Workspace/,
    );
    await expect(
      page.getByTestId("center-content-tab").first(),
    ).toHaveAttribute("aria-selected", "true");

    // Phase 8 rail order and Activity navigation use the real sidebar.
    const sidebar = page.getByTestId("left-sidebar");
    await expect
      .poll(() =>
        sidebar
          .locator("[data-section-id]")
          .evaluateAll((sections) =>
            sections.map((section) => section.getAttribute("data-section-id")),
          ),
      )
      .toEqual([
        // The Overview control lives in the ribbon; the rail starts with
        // the Guided section, which joins the alignment layout behind its
        // own gate
        // (g5.workbench.journey).
        ...(GUIDED_JOURNEY_ENABLED ? ["guided"] : []),
        "ontology",
        "project-docs",
        "files",
        "issues",
        "tasks",
        "all-docs",
        "source-control",
      ]);
    await expect(
      sidebar.getByRole("button", { name: "Tasks", exact: true }),
    ).toBeVisible();
    // The built-in ribbon offers no Analyze menu (user instruction, 2026-10-29).
    await expect(page.getByTestId("ribbon-action-analyze")).toHaveCount(0);
    await page.getByTestId("ribbon-workspace-approvals").click();
    await expect(page.getByTestId("workspace-activity-panel")).toHaveCount(0);
    // The rail has no Overview row; the ribbon's Overview control, before
    // the phase's first action, opens the full page (g5.workbench.landing).
    await expect(sidebar.getByTestId("sidebar-activity-tab")).toHaveCount(0);
    const overview = page.getByTestId("ribbon-overview");
    const overviewBox = (await overview.boundingBox())!;
    const newDocBox = (await page
      .getByTestId("ribbon-action-new-doc")
      .boundingBox())!;
    expect(overviewBox.x).toBeLessThan(newDocBox.x);
    await overview.click();
    await expect(page.getByTestId("workspace-activity-panel")).toBeVisible();
    await expect(
      page.getByTestId("center-content-tab").first(),
    ).toHaveAttribute("aria-selected", "true");

    await expect(
      page.getByRole("heading", { name: "Activity", exact: true }),
    ).toBeVisible();
    await page.getByTestId("ribbon-workspace-approvals").click();
    await expect(
      page.getByRole("heading", { name: /^Approvals Required to merge into / }),
    ).toBeVisible();
    await expect(
      page.getByTestId("workspace-approvals-phase-filter"),
    ).toHaveCount(0);
    // The Overview control returns to the page.
    await page.getByTestId("ribbon-overview").click();
    await expect(page.getByTestId("workspace-activity-panel")).toBeVisible();

    // The owner moves the phase; the move is durable
    // (g5.workbench.phases.state).
    await page.getByTestId("phase-chevron-se").click();
    await expect(page.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(chevrons).toHaveAttribute("data-phase", "se");
    // The durable phase event itself is never a timeline row
    // (g5.workbench.landing, 2026-10-15).
    await expect(page.getByTestId(/^activity-entry-phase:/)).toHaveCount(0);
    // All is a distinct reset state. Clicking a category isolates its history;
    // clicking it again returns to All without re-querying.
    const filters = page.getByTestId("activity-filters");
    await expect(filters).toBeVisible();
    await expect(filters.locator("button[aria-pressed]")).toHaveCount(8);
    await expect(filters.locator("button[aria-pressed=true]")).toHaveCount(1);
    await expect(page.getByTestId("activity-filter-phase_changes")).toHaveCount(
      0,
    );
    // Isolating a category releases All and pressing it again restores it. A
    // fresh clone may have no visible row at all (its phase move is hidden), so
    // the row round trip runs only for a group that has loaded rows; the
    // unit suite covers the hiding itself.
    const groupCounts = await Promise.all(
      ACTIVITY_FILTER_GROUP_IDS.map(async (group) => ({
        group,
        count: Number(
          await page.getByTestId(`activity-filter-count-${group}`).innerText(),
        ),
      })),
    );
    const toggled =
      groupCounts.find(({ count }) => count > 0) ?? groupCounts[0];
    const rows = page
      .getByTestId("workspace-activity-entries")
      .getByRole("listitem");
    const rowsBefore = await rows.count();
    const currentCount = await page
      .getByTestId("workspace-activity-entries")
      .locator('[data-current="true"]')
      .count();
    const groupFilter = page.getByTestId(`activity-filter-${toggled.group}`);
    await groupFilter.click();
    await expect(groupFilter).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByTestId("activity-filter-all")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    await expect(rows).toHaveCount(currentCount + toggled.count);
    const other = groupCounts.find(({ group }) => group !== toggled.group)!;
    const otherFilter = page.getByTestId(`activity-filter-${other.group}`);
    await otherFilter.click();
    await expect(otherFilter).toHaveAttribute("aria-pressed", "true");
    await expect(rows).toHaveCount(currentCount + toggled.count + other.count);
    await otherFilter.click();
    await expect(rows).toHaveCount(currentCount + toggled.count);
    await groupFilter.click();
    await expect(page.getByTestId("activity-filter-all")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(rows).toHaveCount(rowsBefore);

    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await expect(page.getByTestId("phase-chevrons")).toHaveAttribute(
      "data-phase",
      "se",
      { timeout: 15_000 },
    );
    await expect(page.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(page.getByTestId("action-ribbon")).toHaveAttribute(
      "data-phase",
      "se",
    );
    // The chat persona pill rides an open session; the Build mock flow
    // below covers it (g5.workbench.persona).
  });

  // Home's Workspaces tile (g5.ui.home-workspaces): the clone appears among
  // the caller's own rows, its name is the Open link, the Approvals cell and the
  // owner come from the row's reads, and Home offers no phase buttons.
  test("Home lists the clone in the Workspaces tile with approvals, owner and actions, and reads filter and sort from the URL", async ({
    page,
  }) => {
    await page.goto(`/${TEST_ORG}/home`, { waitUntil: "domcontentloaded" });
    const tile = page.getByTestId("home-workspaces");
    await expect(tile).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("home-phase-stepper")).toHaveCount(0);
    await expect(
      tile.getByTestId("home-workspaces-filter-mine"),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(tile.getByTestId("home-workspaces-sort")).toHaveValue(
      "opened",
    );
    // Never-opened workspaces sort by their last update, so a parallel test
    // can legitimately put a newer workspace ahead of this clone. Search for
    // our own row rather than depending on shared organization-wide ordering.
    await tile
      .getByTestId("home-workspace-search")
      .fill(isolatedWorkspace.name);
    const row = tile.getByTestId(`home-workspace-${isolatedWorkspace.id}`);
    await expect(row).toBeVisible({ timeout: 15_000 });
    await expect(row.getByTestId("home-workspace-open")).toHaveAttribute(
      "href",
      isolatedWorkspace.path,
    );
    // The Approvals cell resolves to gates or to the explicit "nothing
    // required" slot, never to the loading placeholder.
    await expect(
      row.locator(
        '[data-testid="home-workspace-approvals"], [data-testid="home-workspace-approvals-none"]',
      ),
    ).toHaveCount(1, { timeout: 15_000 });
    // The owner is the signed-in person, named as the greeting names them.
    const greeting = await page.getByText(/^Hi, /).first().textContent();
    await expect(row.getByTestId("home-workspace-owner")).toContainText(
      (greeting ?? "").replace(/^Hi,\s*/, "").trim(),
    );
    // Delete and Fork for the owner (g5.ui.home-workspaces); Rename lives in
    // Workspace details.
    await expect(row.getByTestId("home-workspace-remove")).toBeVisible();
    await expect(row.getByTestId("home-workspace-rename")).toHaveCount(0);
    await expect(row.getByTestId("home-workspace-fork")).toBeVisible();

    // All + Name from the URL: Base joins the list and the sort is selected.
    await page.goto(`/${TEST_ORG}/home?workspaces=all&sort=name`, {
      waitUntil: "domcontentloaded",
    });
    await expect(
      page.getByTestId("home-workspaces-filter-all"),
    ).toHaveAttribute("aria-pressed", "true", { timeout: 15_000 });
    await expect(page.getByTestId("home-workspaces-sort")).toHaveValue("name");
    // Only a page of rows renders: search for each target rather than
    // assuming it sits on the first alphabetical page.
    const all = page.getByTestId("home-workspaces");
    await all.getByTestId("home-workspace-search").fill(TEST_WORKSPACE);
    await expect(
      all.getByRole("link", { name: `Open ${TEST_WORKSPACE}` }),
    ).toBeVisible({ timeout: 15_000 });
    await all.getByTestId("home-workspace-search-clear").click();
    await all.getByTestId("home-workspace-search").fill(isolatedWorkspace.name);

    // The name opens the workspace as a fresh open.
    await all
      .getByRole("link", { name: `Open ${isolatedWorkspace.name}` })
      .click();
    await waitForWorkspaceLoad(page);
    await expect(page).toHaveURL(new RegExp(`${isolatedWorkspace.path}`));
  });

  test.describe(() => {
    test.describe.configure({ timeout: 420_000 });
    // Under `alignment` Home renders four tiles — Workspaces, Needs Your
    // Attention, Issues and tasks, Running compute (g5.ui.home-workspaces) and
    // no full issue search. The compute tile reads the member-visible inventory
    // (`GET /orgs/:id/compute/running`); the clone from beforeEach is started
    // here so its row appears, and Stop from Home drives the owner stop
    // (g5.lifecycle.stop) until the lifecycle reports the VM stopped.
    test("Home shows the four alignment tiles and stops the running clone from the compute tile", async ({
      page,
    }) => {
      await runWorkspaceVmAction(page, isolatedWorkspace.id, "resume");
      await waitForWorkspaceVmStatus(page, isolatedWorkspace.id, "running");

      await page.goto(`/${TEST_ORG}/home`, { waitUntil: "domcontentloaded" });
      const columns = page.getByTestId("home-columns");
      // Home has no top bar: the Workspaces tile carries search and fork,
      // the strip the global controls.
      const workspaces = columns.getByTestId("home-workspaces");
      await expect(workspaces).toBeVisible({ timeout: 15_000 });
      await workspaces
        .getByTestId("home-workspace-search")
        .fill(isolatedWorkspace.name);
      await expect(
        workspaces.getByTestId(`home-workspace-${isolatedWorkspace.id}`),
      ).toBeVisible({ timeout: 15_000 });
      await expect(workspaces.getByTestId("home-workspace-open")).toHaveCount(
        1,
      );
      await workspaces.getByTestId("home-workspace-search-clear").click();
      await expect(
        page.getByTestId("shell-controls").getByTestId("org-management-button"),
      ).toBeVisible();
      await expect(columns.getByTestId("home-needs-attention")).toBeVisible({
        timeout: 15_000,
      });
      const compute = columns.getByTestId("home-compute-tile");
      await expect(compute).toBeVisible();
      await expect(
        compute.getByTestId("home-compute-filter-mine"),
      ).toHaveAttribute("aria-pressed", "true");
      // The Issues tile needs the external-issues feature; the stack may run
      // without a GitHub App.
      const issuesTile = columns.getByTestId("home-issues-tile");
      const issuesPresent = await issuesTile
        .waitFor({ state: "visible", timeout: 5_000 })
        .then(() => true)
        .catch(() => false);
      if (issuesPresent) {
        await expect(
          issuesTile.getByRole("heading", { name: "Issues and tasks" }),
        ).toBeVisible();
        await expect(
          issuesTile.getByTestId("home-issues-filter-me"),
        ).toHaveAttribute("aria-pressed", "true");
        // Create is always shown (g5.workbench.issues.create); without a ready
        // creation target (the usual e2e stack) it is disabled with the reason.
        await expect(
          issuesTile.getByTestId("home-issues-view-all"),
        ).toHaveCount(0);
        const create = issuesTile.getByTestId("home-issues-create");
        await expect(create).toBeVisible();
        await expect(create).toHaveAttribute("title", /\S/);
        if (await create.isEnabled()) {
          await create.click();
          const dialog = page.getByTestId("create-issue-dialog");
          await expect(dialog.getByTestId("create-issue-title")).toHaveValue(
            "",
          );
          await expect(
            dialog.getByTestId("create-issue-destination"),
          ).toContainText("Creating in");
          await dialog.getByTestId("create-issue-cancel").click();
          await expect(dialog).toHaveCount(0);
        }
      }

      // The tile pages five at a time: reveal pages until the clone's row shows.
      const row = compute.getByTestId(`home-compute-${isolatedWorkspace.id}`);
      for (let page = 0; page < 20 && (await row.count()) === 0; page += 1) {
        const more = compute.getByTestId("home-compute-show-more");
        if ((await more.count()) === 0) break;
        await more.click();
      }
      await expect(row).toBeVisible({ timeout: 15_000 });
      await expect(row).toHaveAttribute("data-status", "running");
      await expect(row.getByTestId("home-compute-details")).toContainText(
        /vCPU|up /,
      );
      await expect(row.getByTestId("home-compute-open")).toHaveAttribute(
        "href",
        isolatedWorkspace.path,
      );
      const shutDown = row.getByTestId("home-compute-shutdown");
      await expect(shutDown).toBeEnabled();
      // The stop request settles only once the VM is stopped and the row then
      // leaves the inventory, so the "shutting down" state is asserted while
      // the request is still in flight.
      const stopResponsePromise = page.waitForResponse(
        (response) =>
          response
            .url()
            .includes(`/api/v1/workspaces/${isolatedWorkspace.id}/stop`) &&
          response.request().method() === "POST",
      );
      await shutDown.click();
      await expect(shutDown).toBeDisabled();
      await expect(shutDown).toHaveText(/Stopping…/);
      await expect(row).toHaveAttribute("data-status", "stopping");
      const stopResponse = await stopResponsePromise;
      expect(stopResponse.status()).toBe(200);
      // The row leaves the inventory once the lifecycle reports the VM stopped.
      await expect(row).toHaveCount(0, { timeout: 240_000 });
      const vm = await waitForWorkspaceVmStatus(
        page,
        isolatedWorkspace.id,
        "stopped",
      );
      expect(vm.status).toBe("stopped");
      await page.reload({ waitUntil: "domcontentloaded" });
      await expect(columns.getByTestId("home-compute-tile")).toBeVisible({
        timeout: 15_000,
      });
      await expect(
        columns.getByTestId(`home-compute-${isolatedWorkspace.id}`),
      ).toHaveCount(0);
    });
  });

  // g5.workbench.header-styles: the Phase bar and Action bar settings change
  // how the live header renders and survive a reload; the vertical phase bar
  // sits in the same row as the ribbon and icon-only buttons keep their
  // label as the accessible name.
  test("Phase bar and Action bar settings restyle the live workbench header", async ({
    page,
  }) => {
    await openWorkspaceSettingsPage(page);
    const phaseBar = page.getByTestId("phase-bar-style-select");
    const actionBar = page.getByTestId("action-bar-style-select");
    await expect(phaseBar).toHaveValue("minimal");
    await expect(actionBar).toHaveValue("dynamic");
    await phaseBar.selectOption("vertical");
    await actionBar.selectOption("minimal-icons");
    await expect(phaseBar).toHaveValue("vertical");
    await expect(actionBar).toHaveValue("minimal-icons");

    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    const header = page.getByTestId("workbench-header");
    await expect(header).toBeVisible({ timeout: 15_000 });
    await expect(header).toHaveAttribute("data-phase-bar", "vertical");
    await expect(header).toHaveAttribute("data-action-bar", "minimal-icons");
    const chevrons = page.getByTestId("phase-chevrons");
    await expect(chevrons).toHaveAttribute("data-variant", "vertical");
    await expect(chevrons).toContainText("Business Engineering");
    await expect(chevrons).toContainText("Integration Engineering");
    await expect(chevrons).not.toContainText("Requirement doc");
    const ribbon = page.getByTestId("action-ribbon");
    await expect(ribbon).toHaveAttribute("data-variant", "minimal-icons");
    const build = page.getByTestId("ribbon-action-build-mock");
    await expect(build).toHaveAttribute("aria-label", "Build mock");
    // The icon-only style names the button in its tooltip; a stopped
    // workspace appends the reason the chat action is greyed.
    await expect(build).toHaveAttribute("title", /^Build mock/);
    await expect(build).toHaveText("");
    await expect(page.getByTestId("ribbon-branch")).toHaveCount(0);
    // Chevron column and ribbon share one row.
    const chevronBox = await chevrons.boundingBox();
    const ribbonBox = await ribbon.boundingBox();
    expect(chevronBox && ribbonBox && chevronBox.x < ribbonBox.x).toBe(true);
    expect(
      chevronBox && ribbonBox && Math.abs(chevronBox.y - ribbonBox.y) < 4,
    ).toBe(true);

    // The move still works from the column, and the phase survives reload.
    await page.getByTestId("phase-chevron-se").click();
    await expect(page.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-pressed",
      "true",
      { timeout: 15_000 },
    );
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await expect(page.getByTestId("phase-chevrons")).toHaveAttribute(
      "data-phase",
      "se",
      { timeout: 15_000 },
    );
    await expect(page.getByTestId("action-ribbon")).toHaveAttribute(
      "data-variant",
      "minimal-icons",
    );
  });

  // g5.workbench.header-styles `dynamic`: full extended icons while the row
  // fits, one compact step when it stops fitting, then a horizontally
  // scrolling groups row with the workspace controls pinned at the right and
  // menus positioned outside the scroller. Desktop widths only: below 1024px
  // the page takes its mobile layout, which has no workbench header. The
  // vertical phase column and System Engineering's twelve actions make the
  // three widths land in the three states with margin.
  test("the dynamic Action bar shrinks to compact icons, then scrolls with menus unclipped", async ({
    page,
  }) => {
    await openWorkspaceSettingsPage(page);
    await page.getByTestId("phase-bar-style-select").selectOption("vertical");
    const actionBar = page.getByTestId("action-bar-style-select");
    await actionBar.selectOption("dynamic");
    await expect(actionBar).toHaveValue("dynamic");

    await page.setViewportSize({ width: 1920, height: 900 });
    await openWorkspacePath(page, isolatedWorkspace.path);
    const header = page.getByTestId("workbench-header");
    await expect(header).toHaveAttribute("data-action-bar", "dynamic", {
      timeout: 15_000,
    });
    await page.getByTestId("phase-chevron-se").click();
    const ribbon = page.getByTestId("action-ribbon");
    await expect(ribbon).toHaveAttribute("data-phase", "se", {
      timeout: 15_000,
    });
    await expect(ribbon).toHaveAttribute("data-variant", "dynamic");
    await expect(ribbon).toHaveAttribute("data-density", "full");
    const glyphWidth = (testId: string) =>
      ribbon
        .getByTestId(testId)
        .locator("svg")
        .first()
        .evaluate((svg) => svg.getBoundingClientRect().width);
    expect(await glyphWidth("ribbon-action-generate-code")).toBe(22);
    expect(await glyphWidth("ribbon-workspace-approvals")).toBe(22);

    await page.setViewportSize({ width: 1400, height: 900 });
    await expect(ribbon).toHaveAttribute("data-density", "compact");
    expect(await glyphWidth("ribbon-action-generate-code")).toBe(18);
    expect(await glyphWidth("ribbon-workspace-approvals")).toBe(18);
    // The workspace controls take the compact shape too.
    expect(await glyphWidth("workspace-info-button")).toBe(18);
    const groups = ribbon.getByTestId("ribbon-groups");
    expect(
      await groups.evaluate((el) => el.scrollWidth > el.clientWidth + 1),
    ).toBe(false);

    await page.setViewportSize({ width: 1024, height: 900 });
    await expect
      .poll(() => groups.evaluate((el) => el.scrollWidth > el.clientWidth + 1))
      .toBe(true);
    await expect(ribbon).toHaveAttribute("data-density", "compact");
    // The controls stay pinned at the right end, outside the scroller.
    const details = ribbon.getByTestId("workspace-info-button");
    await expect(details).toBeInViewport({ ratio: 1 });
    const last = ribbon.getByTestId("ribbon-workspace-approvals");
    await last.scrollIntoViewIfNeeded();
    await expect(last).toBeInViewport({ ratio: 1 });
    await last.click({ trial: true });
    const groupsBox = (await groups.boundingBox())!;
    const lastBox = (await last.boundingBox())!;
    expect(lastBox.x + lastBox.width).toBeLessThanOrEqual(
      groupsBox.x + groupsBox.width + 1,
    );

    // A menu opened inside the scroller is not clipped by it.
    await groups.evaluate((el) => {
      el.scrollLeft = 0;
    });
    const trigger = ribbon.getByTestId("ribbon-action-import");
    await trigger.click();
    const menu = page.getByTestId("ribbon-menu-import");
    await expect(menu).toBeVisible();
    await expect(menu).toHaveCSS("position", "fixed");
    await expect(menu).toBeInViewport({ ratio: 1 });
    // The menu hangs from its trigger; a one-line trigger ends above the
    // group caption, so the row's bottom edge is not the anchor.
    const triggerBox = (await trigger.boundingBox())!;
    const menuBox = (await menu.boundingBox())!;
    expect(menuBox.y).toBeGreaterThan(triggerBox.y + triggerBox.height - 1);
    expect(
      await menu.evaluate((el) => {
        const box = el.getBoundingClientRect();
        return el.contains(
          document.elementFromPoint(box.left + box.width / 2, box.bottom - 6),
        );
      }),
    ).toBe(true);
    // Scrolling the row closes it, since its position would go stale.
    await groups.evaluate((el) => {
      el.scrollLeft = 60;
    });
    await expect(menu).toHaveCount(0);

    await page.setViewportSize({ width: 1920, height: 900 });
    await expect(ribbon).toHaveAttribute("data-density", "full");
    expect(await glyphWidth("ribbon-action-generate-code")).toBe(22);
    expect(await glyphWidth("ribbon-workspace-approvals")).toBe(22);
  });

  // Workbench Phase 2 (g5.workbench.actions, g5.workbench.persona): a ribbon
  // `chat` action starts a persona session whose first queued input is the
  // persona instructions plus the template; an `open` action reaches the
  // existing surface (g5.workbench.ship-actions: Rebase → the Rebase
  // Workspace dialog, Create PR → PrepareToShipModal).
  test("Build mock seeds the composer; the send starts a BE chat under the persona; Rebase opens the rebase dialog and Create PR the ship modal", async ({
    page,
  }) => {
    // Chat actions seed the composer, which sends only in a running
    // workspace (g5.workbench.chat-actions).
    await runWorkspaceVmAction(page, isolatedWorkspace.id, "resume");
    await waitForWorkspaceVmStatus(page, isolatedWorkspace.id, "running");
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    const ribbon = page.getByTestId("action-ribbon");
    await expect(ribbon).toBeVisible({ timeout: 15_000 });
    await expect(ribbon).toHaveAttribute(
      "data-config-status",
      /builtin|committed/,
      {
        timeout: 15_000,
      },
    );

    const config = await authedApi(
      page,
      `/api/v1/workspaces/${isolatedWorkspace.id}/workbench`,
    );
    expect(config.status).toBe(200);
    const bePersona: string = config.body.data.personas.be.text;

    const build = page.getByTestId("ribbon-action-build-mock");
    await expect(build).toBeEnabled({ timeout: 15_000 });
    // Build mock is one dialog (g5.workbench.mock): it lists the mocks under
    // Project Docs mocks/ (none yet) and its primary action seeds the
    // composer and pulses Send; nothing is sent until the person sends
    // (g5.workbench.chat-actions).
    await build.click();
    const mockDialog = page.getByRole("dialog", { name: "Build mock" });
    await expect(mockDialog).toBeVisible({ timeout: 15_000 });
    await expect(mockDialog.getByTestId("build-mock-empty")).toBeVisible();
    await expect(page.getByTestId("ribbon-action-preview-mock")).toHaveCount(0);
    const submit = mockDialog.getByTestId("build-mock-submit");
    await expect(submit).toBeEnabled({ timeout: 15_000 });
    await submit.click();
    await expect(mockDialog).not.toBeVisible();
    const composer = page.getByTestId("chat-input").first();
    await expect(composer).toHaveValue(/Build a clickable mock/, {
      timeout: 15_000,
    });
    const send = page.getByTestId("chat-panel-no-session-send");
    // Send pulses once it can send (the launch resolution settles first).
    await expect(send).toBeEnabled({ timeout: 15_000 });
    await expect(send).toHaveAttribute("data-pulse", "true");
    const [startResponse] = await Promise.all([
      page.waitForResponse(
        (response) =>
          response.url().includes("/sessions/start") &&
          response.request().method() === "POST",
      ),
      send.click(),
    ]);
    const startRequest = startResponse.request().postDataJSON();
    expect(startRequest.persona).toBe("be");
    expect(startRequest.actionId).toBeUndefined();
    expect(startRequest.content).toContain("Build a clickable mock");
    expect(startRequest.configRevision).toBe(config.body.data.revision);
    expect(startResponse.status()).toBe(201);
    const started = (await startResponse.json()).data;
    expect(started.session.persona).toBe("be");

    // The first queued input is the persona prefix plus the sent text.
    const inputs = await authedApi(
      page,
      `/api/v1/sessions/${started.session.id}/inputs`,
    );
    expect(inputs.status).toBe(200);
    const queued: Array<{ content: string }> = inputs.body.data ?? inputs.body;
    expect(queued.length).toBeGreaterThan(0);
    expect(queued[0].content.startsWith(bePersona)).toBeTruthy();
    expect(queued[0].content).toContain("Build a clickable mock");

    // The chat opens on the new session with its persona pill.
    const pill = page.getByTestId("chat-phase-pill");
    await expect(pill).toHaveAttribute("data-source", "persona", {
      timeout: 15_000,
    });
    await expect(pill).toHaveText("BE");

    // Rebase (SE) opens the source-only Rebase Workspace dialog.
    await page.getByTestId("phase-chevron-se").click();
    await expect(page.getByTestId("ribbon-action-create-pr")).toHaveCount(0);
    const rebase = page.getByTestId("ribbon-action-rebase");
    await expect(rebase).toBeEnabled({ timeout: 15_000 });
    await rebase.click();
    await expect(
      page.getByRole("heading", { name: /Rebase Workspace/ }),
    ).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("workspace-merge-modal-close").click();
    await expect(
      page.getByRole("heading", { name: /Rebase Workspace/ }),
    ).toHaveCount(0);

    // Create PR (IE) opens the existing ship modal.
    await page.getByTestId("phase-chevron-ie").click();
    await expect(page.getByTestId("ribbon-action-rebase")).toHaveCount(0);
    const createPr = page.getByTestId("ribbon-action-create-pr");
    await expect(createPr).toBeEnabled({ timeout: 15_000 });
    await expect(createPr).toHaveText(/Create PR/);
    await createPr.click();
    await expect(
      page.getByRole("heading", { name: /Prepare to Ship/ }),
    ).toBeVisible({ timeout: 15_000 });
    await page.getByTestId("prepare-to-ship-close-button").click();
  });

  test.describe(() => {
    test.describe.configure({ timeout: 170_000 });
    // Workbench Phase 4 (g5.workbench.meeting): Review meeting opens one
    // dialog; uploading a transcript stores it as a Markdown Project Doc under
    // `meetings/` and starts the BE extraction template over it; the fake
    // provider's meeting fixture creates the question set through
    // `g5d question create` and writes the decisions document, whose cards
    // carry the three meeting options; the task options are disabled with the
    // reason while no issue tracker is connected, and Reject is the owner's
    // Answer (a decision, published by the store's explicit save).
    test("Review meeting: an uploaded transcript is extracted into decision cards, task options use the G5 tracker and Reject decides", async ({
      page,
    }) => {
      let turn: ChatTurnIdentity | undefined;
      try {
        await runWorkspaceVmAction(page, isolatedWorkspace.id, "resume");
        await waitForWorkspaceVmStatus(page, isolatedWorkspace.id, "running");
        await openWorkspacePath(page, isolatedWorkspace.path);
        // The workbench lands on Activity; Chat is a center tab.
        await page.getByTestId("center-chat-tab").click();
        await selectAgentProviderViaSettings(page, "claude_code");
        await expectSelectedAgentProvider(page, "claude_code");

        const ribbon = page.getByTestId("action-ribbon");
        await expect(ribbon).toHaveAttribute(
          "data-config-status",
          /builtin|committed/,
          { timeout: 15_000 },
        );
        const review = page.getByTestId("ribbon-action-review-meeting");
        await expect(review).toBeEnabled({ timeout: 15_000 });
        await review.click();
        const dialog = page.getByRole("dialog");
        await expect(dialog).toContainText("Review meeting");
        await expect(
          dialog.getByTestId("transcript-source-existing"),
        ).toBeVisible();
        const transcriptName = `ui-discussion-${Date.now()}`;
        await dialog.getByLabel("Upload a transcript").setInputFiles({
          name: `${transcriptName}.txt`,
          mimeType: "text/plain",
          buffer: Buffer.from(
            "Tim 00:28: the project timeline becomes the landing page.\nTim 00:19: ribbon buttons are templated chats.\n",
          ),
        });
        const transcriptPath = `docs/${isolatedWorkspace.id}/meetings/${transcriptName}.md`;
        await dialog
          .getByRole("button", { name: "Upload and extract decisions" })
          .click();
        // Upload plus extraction is one user action up to the composer: the
        // BE extraction prompt is seeded with the stored transcript as its
        // current document, and the person sends it
        // (g5.workbench.chat-actions).
        const composer = page.getByTestId("chat-input").first();
        await expect(composer).toHaveValue(
          new RegExp(`Context: the current document is \`${transcriptPath}\``),
          { timeout: 15_000 },
        );
        const send = page.getByTestId("chat-panel-no-session-send");
        await expect(send).toBeEnabled({ timeout: 15_000 });
        await expect(send).toHaveAttribute("data-pulse", "true");
        const [startResponse] = await Promise.all([
          page.waitForResponse(
            (response) =>
              response.url().includes("/sessions/start") &&
              response.request().method() === "POST",
          ),
          send.click(),
        ]);
        const startRequest = startResponse.request().postDataJSON();
        expect(startRequest.persona).toBe("be");
        expect(startRequest.actionId).toBeUndefined();
        expect(startRequest.content).toContain(transcriptPath);
        expect(startResponse.status(), await startResponse.text()).toBe(201);
        turn = await captureChatTurn(page, startResponse, (created) => {
          turn = created;
        });
        // Read the reply from the session so a stalled turn fails within the
        // test budget with the task's own evidence.
        const replied = Date.now() + 90_000;
        let messages = "";
        while (!messages.includes("Fake meeting decisions recorded")) {
          if (Date.now() > replied) {
            const task = await authedApi(page, `/api/v1/tasks/${turn.taskId}`);
            const timeline = await authedApi(
              page,
              `/api/v1/tasks/${turn.taskId}/timeline`,
            );
            throw new Error(
              `no meeting reply in 90s; task ${JSON.stringify(task.body?.data ?? task.body).slice(0, 1500)}; timeline ${JSON.stringify(timeline.body).slice(-3000)}; messages ${messages.slice(-3000)}`,
            );
          }
          await page.waitForTimeout(2_000);
          const listed = await authedApi(
            page,
            `/api/v1/sessions/${turn.sessionId}/messages`,
          );
          messages = JSON.stringify(listed.body);
          const failed = messages.match(
            /Fake meeting extraction failed: [^"]*/,
          );
          if (failed) throw new Error(failed[0]);
        }

        // The set holds one question per decision with the three options.
        const setPath = `docs/${isolatedWorkspace.id}/questions/meeting-${transcriptName}.g5q.json`;
        const set = await authedApi(
          page,
          `/api/v1/workspaces/${isolatedWorkspace.id}/questions/set?path=${encodeURIComponent(setPath)}`,
        );
        expect(set.status, JSON.stringify(set.body)).toBe(200);
        const questions = (
          set.body as {
            data: {
              questions: Array<{
                id: string;
                status: string;
                options: Array<{ id: string }>;
              }>;
            };
          }
        ).data.questions;
        expect(questions).toHaveLength(2);
        expect(questions[0].status).toBe("open");
        expect(questions[0].options.map((option) => option.id)).toEqual([
          "attach",
          "detach",
          "reject",
        ]);

        // The decisions document renders the cards; on a meeting card an
        // option press is the owner's Answer (one press). Without an issue
        // tracker the task options are disabled with the reason
        // (g5.workbench.meeting, g5.ui.unavailable-actions).
        const decisionsPath = `docs/${isolatedWorkspace.id}/meetings/${transcriptName}.decisions.md`;
        const docId = await docIdForPath(
          page,
          isolatedWorkspace.id,
          decisionsPath,
        );
        await page.goto(
          `${isolatedWorkspace.path}/doc/${encodeURIComponent(docId)}`,
        );
        const cards = page.getByTestId("referenced-question-card");
        await expect(cards).toHaveCount(2, { timeout: 60_000 });
        const first = cards.first();
        const attach = first.locator(
          '[data-testid="referenced-question-option"][data-option-id="attach"]',
        );
        // Every organization tracks issues with G5 issues from the start
        // (g5.issue.native.domain), so the task options are available.
        await expect(attach).toBeEnabled();
        await expect(
          first.getByTestId("referenced-question-vote-controls"),
        ).toHaveCount(0);
        await first
          .locator(
            '[data-testid="referenced-question-option"][data-option-id="reject"]',
          )
          .click();
        await expect(
          first.getByTestId("referenced-question-saved"),
        ).toBeVisible({
          timeout: 60_000,
        });
        const decided = await authedApi(
          page,
          `/api/v1/workspaces/${isolatedWorkspace.id}/questions/set?path=${encodeURIComponent(setPath)}`,
        );
        const entry = (
          decided.body as {
            data: {
              questions: Array<{
                decision: boolean;
                answer: {
                  selectedOptionIds: string[];
                  answeredBy: { kind: string } | null;
                };
              }>;
            };
          }
        ).data.questions[0];
        expect(entry.decision).toBe(true);
        expect(entry.answer.selectedOptionIds).toEqual(["reject"]);
        expect(entry.answer.answeredBy?.kind).toBe("user");
      } finally {
        if (turn) await releaseChatTurn(page, turn).catch(() => {});
      }
    });
  });

  // Workbench Phase 3 (g5.workbench.gates, g5.governance.approval.requirements):
  // a definition bound to BE and IE meters both chevrons from the phase DTO,
  // the shared rail opens Approvals and its page owns phase filtering,
  // a BE decision settles only the BE meter, and the IE requirement alone
  // refuses the merge with the server's admission reason.
  test("workspace-wide approvals survive phase moves and gate only target merge", async ({
    page,
    browser,
  }) => {
    const { orgId } = await readCurrentOrgContext(page);
    const targetId = await resolveWorkspaceId(page, TEST_ORG, TEST_WORKSPACE);
    const { actor: reviewer, context } = await createReviewActor(
      page,
      browser,
      orgId,
      "Phase reviewer",
    );
    let definitionId: string | undefined;
    try {
      const definitionName = `E2E phased sign-off ${Date.now()}`;
      const created = await authedApi(
        page,
        `/api/v1/orgs/${orgId}/approval-definitions`,
        "POST",
        {
          name: definitionName,
          humanUserIds: [reviewer.id],
          phases: ["be", "ie"],
        },
      );
      expect(created.status, JSON.stringify(created.body)).toBe(201);
      expect(created.body.data.phases).toEqual(["be", "ie"]);
      definitionId = created.body.data.id;
      const attached = await authedApi(
        page,
        `/api/v1/workspaces/${targetId}/approvals/requirements`,
        "POST",
        { definitionId },
      );
      expect(attached.status, JSON.stringify(attached.body)).toBe(200);

      const pr = await authedApi(
        page,
        `/api/v1/workspaces/${isolatedWorkspace.id}/pull-requests`,
        "POST",
        {
          targetWorkspaceId: targetId,
          description: "Phase-scoped requested approvals",
        },
      );
      expect(pr.status, JSON.stringify(pr.body)).toBe(200);
      const requestIds: Partial<Record<"be" | "ie", string>> = {};
      for (const phase of ["be", "ie"] as const) {
        const requested = await authedApi(
          page,
          `/api/v1/workspaces/${isolatedWorkspace.id}/approvals/requests`,
          "POST",
          {
            definitionId,
            definitionVersion: 1,
            phase,
            actorUserIds: [reviewer.id],
          },
        );
        expect(requested.status, JSON.stringify(requested.body)).toBe(
          phase === "be" ? 201 : 200,
        );
        requestIds[phase] = requested.body.data.requests[0].requestId;
        expect(requestIds[phase]).toBeTruthy();
      }
      expect(requestIds.be).toBe(requestIds.ie);
      await page.goto(isolatedWorkspace.path, {
        waitUntil: "domcontentloaded",
      });
      await waitForWorkspaceLoad(page);

      // Approval state belongs to the workspace; phase controls only move phases.
      const chevrons = page.getByTestId("phase-chevrons");
      await expect(chevrons.getByTestId(/^phase-approval-meter-/)).toHaveCount(
        0,
      );
      const headerApprovals = page.getByTestId("landing-summary-approvals");
      await expect(headerApprovals).toHaveAttribute("data-state", "pending");
      await expect(headerApprovals).toContainText(
        "0 approved, 0 rejected, 1 pending",
      );

      // The merge is refused through the UI while IE is pending: Merge to
      // parent opens the ship surface with the server's admission reason.
      await page.getByTestId("phase-chevron-ie").click();
      const merge = page.getByTestId("ribbon-action-merge");
      await expect(merge).toBeEnabled({ timeout: 15_000 });
      await merge.click();
      await expect(
        page.getByRole("heading", { name: /Prepare to Ship/ }),
      ).toBeVisible({ timeout: 15_000 });
      await expect(
        page.getByTestId("prepare-to-ship-ship-button"),
      ).toBeDisabled();
      await expect(
        page.getByTestId("prepare-to-ship-approvals-counts"),
      ).toHaveText("0 approved, 0 rejected, 1 pending");
      await expect(page.getByTestId("prepare-to-ship-ship-note")).toContainText(
        "Approval(s) required before merging",
      );
      // The blocker never repeats requirement names.
      await expect(
        page.getByTestId("prepare-to-ship-ship-note"),
      ).not.toContainText(definitionName);
      await page.getByTestId("prepare-to-ship-close-button").click();
      await page.getByTestId("phase-chevron-be").click();

      await expect(page.getByTestId("ribbon-action-approvals")).toHaveCount(0);
      await page.getByTestId("ribbon-workspace-approvals").click();
      await expect(
        page.getByTestId("workspace-approvals-requirement"),
      ).toHaveCount(1);
      await expect(
        page.getByTestId("workspace-approvals-phase-filter"),
      ).toHaveCount(0);
      await page.getByTestId("workspace-approvals-definition").click();
      const definition = page.getByRole("dialog", { name: /definition$/ });
      await expect(definition).toContainText(
        "Recommended for Business Engineering",
      );
      await expect(definition).toContainText(
        "Recommended for Integration Engineering",
      );
      await page.keyboard.press("Escape");
      await expect(definition).not.toBeVisible();

      const reviewed = await authedApi(
        reviewer.page,
        `/api/v1/workspaces/${isolatedWorkspace.id}/review-flow`,
      );
      expect(reviewed.status, JSON.stringify(reviewed.body)).toBe(200);
      const reviewedGitHeads = reviewed.body.data.heads;
      const decided = await authedApi(
        reviewer.page,
        `/api/v1/workspaces/${isolatedWorkspace.id}/approvals/decisions`,
        "POST",
        {
          definitionId,
          requestId: requestIds.be,
          reviewedGitHeads,
          outcome: "approved",
          phase: "be",
        },
      );
      expect(decided.status, JSON.stringify(decided.body)).toBe(200);
      const admission = decided.body.data.outgoing.mergeAdmission;
      expect(admission.admitted).toBe(true);
      expect(admission.requirements).toHaveLength(1);
      expect(admission.requirements[0].definitionId).toBe(definitionId);
      expect(admission.requirements[0].state).toBe("approved");

      // A legacy phase-different replay settles the same task exactly once.
      const shipped = await authedApi(
        reviewer.page,
        `/api/v1/workspaces/${isolatedWorkspace.id}/approvals/decisions`,
        "POST",
        {
          definitionId,
          requestId: requestIds.ie,
          reviewedGitHeads,
          outcome: "approved",
          phase: "ie",
        },
      );
      expect(shipped.status, JSON.stringify(shipped.body)).toBe(200);
      expect(shipped.body.data.outgoing.mergeAdmission.admitted).toBe(true);
      await page.reload({ waitUntil: "domcontentloaded" });
      await waitForWorkspaceLoad(page);
      await page.goto(isolatedWorkspace.path, {
        waitUntil: "domcontentloaded",
      });
      await waitForWorkspaceLoad(page);
      // Requirement counts, never approver tallies
      // (g5.workbench.landing.summary).
      await expect(page.getByTestId("landing-summary-approvals")).toContainText(
        "1 approved, 0 rejected, 0 pending",
      );
      await expect(chevrons.getByTestId(/^phase-approval-meter-/)).toHaveCount(
        0,
      );
      await page.getByTestId("phase-chevron-ie").click();
      const mergeAgain = page.getByTestId("ribbon-action-merge");
      await expect(mergeAgain).toBeEnabled({ timeout: 15_000 });
      await mergeAgain.click();
      await expect(
        page.getByRole("heading", { name: /Prepare to Ship/ }),
      ).toBeVisible({ timeout: 15_000 });
      // ① counts the approved requirement; ④ drops the approval blocker
      // once every requirement is approved (g5.workbench.ship-approvals,
      // g5.workbench.ship-to-parent).
      await expect(
        page.getByTestId("prepare-to-ship-approvals-counts"),
      ).toHaveText("1 approved, 0 rejected, 0 pending");
      await expect(
        page.getByTestId("prepare-to-ship-ship-note"),
      ).not.toContainText("Approval(s) required before merging");
      await page.getByTestId("prepare-to-ship-close-button").click();
    } finally {
      if (definitionId)
        await authedApi(
          page,
          `/api/v1/workspaces/${targetId}/approvals/requirements/${definitionId}/retire`,
          "POST",
          {},
        ).catch(() => {});
      await context.close();
    }
  });

  // g5.ui.home-attention-order(-control): Needs Your Attention opens on
  // Newest, and Oldest orders the whole outstanding list on the server
  // (every Show more page continues it); the choice lives in the URL, so a
  // reload keeps it, and the kind filter keeps it. A self-approvable
  // requirement on the clone's target, an open PR and a review request
  // addressed to the session user give it an approval to decide. Other suites
  // leave and remove attention items on the shared stack throughout the run,
  // so the list can change under this test: the tile is compared with the very
  // responses it rendered, and its rows are checked to be in the chosen order.
  // The server order across pages is covered by api_home.rs.
  test("Needs Your Attention opens on Newest and Oldest orders the whole list through Show more, reload and the filter", async ({
    page,
  }) => {
    const { orgId } = await readCurrentOrgContext(page);
    const targetId = await resolveWorkspaceId(page, TEST_ORG, TEST_WORKSPACE);
    const myUserId = await readActiveSubject(page);
    const created = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/approval-definitions`,
      "POST",
      {
        name: `E2E attention order ${Date.now()}`,
        humanUserIds: [myUserId],
        phases: ["ie"],
        allowSelfApproval: true,
      },
    );
    expect(created.status, JSON.stringify(created.body)).toBe(201);
    const definitionId: string = created.body.data.id;
    const attached = await authedApi(
      page,
      `/api/v1/workspaces/${targetId}/approvals/requirements`,
      "POST",
      { definitionId },
    );
    expect(attached.status, JSON.stringify(attached.body)).toBe(200);
    const pr = await authedApi(
      page,
      `/api/v1/workspaces/${isolatedWorkspace.id}/pull-requests`,
      "POST",
      { targetWorkspaceId: targetId, description: "Attention order review" },
    );
    expect(pr.status, JSON.stringify(pr.body)).toBe(200);
    const requested = await authedApi(
      page,
      `/api/v1/workspaces/${isolatedWorkspace.id}/approvals/requests`,
      "POST",
      {
        definitionId,
        definitionVersion: 1,
        phase: "ie",
        actorUserIds: [myUserId],
      },
    );
    expect([200, 201], JSON.stringify(requested.body)).toContain(
      requested.status,
    );

    type Order = "newest" | "oldest";
    type Kind = "approval" | null;
    interface AttentionResponse {
      order: string | null;
      kind: string | null;
      cursor: string | null;
      times: string[];
      nextCursor: string | null;
    }
    // Every attention page the browser fetched since the last reset. The
    // test reads the API itself only before recording starts.
    let responses: AttentionResponse[] = [];
    const record = async (response: Response) => {
      const url = new URL(response.url());
      if (!url.pathname.endsWith("/home/attention") || !response.ok()) return;
      const body = await response.json().catch(() => null);
      const data = body?.data as
        | {
            items: Array<{ actionableAt: string }>;
            nextCursor: string | null;
          }
        | undefined;
      if (!data) return;
      responses.push({
        order: url.searchParams.get("order"),
        kind: url.searchParams.get("kind"),
        cursor: url.searchParams.get("cursor"),
        times: data.items.map((item) => item.actionableAt),
        nextCursor: data.nextCursor,
      });
    };
    /** The times the tile fetched for one order and kind: its latest first
     * page, continued by the latest fetch of each next page. */
    const fetchedTimes = (order: Order, kind: Kind): string[] => {
      const latest = (cursor: string | null) =>
        responses
          .filter(
            (response) =>
              response.order === order &&
              response.kind === kind &&
              response.cursor === cursor,
          )
          .at(-1);
      const times: string[] = [];
      let fetched = latest(null);
      while (fetched) {
        times.push(...fetched.times);
        fetched = fetched.nextCursor ? latest(fetched.nextCursor) : undefined;
      }
      return times;
    };
    const inOrder = (times: string[], order: Order) =>
      times.every((time, index) => {
        if (index === 0) return true;
        const previous = Date.parse(times[index - 1]);
        const current = Date.parse(time);
        return order === "newest" ? previous >= current : previous <= current;
      });

    try {
      // The clone's requested approval is outstanding (other items may be
      // newer, so it need not be on page one).
      await expect
        .poll(
          async () => {
            const response = await authedApi(
              page,
              `/api/v1/orgs/${orgId}/home/attention?limit=50&order=newest&kind=approval`,
            );
            return (
              response.body.data.items as Array<{ workspaceId: string }>
            ).filter((item) => item.workspaceId === isolatedWorkspace.id)
              .length;
          },
          { timeout: 30_000 },
        )
        .toBe(1);

      page.on("response", record);
      await page.goto(`/${TEST_ORG}/home`, { waitUntil: "domcontentloaded" });
      const tile = page.getByTestId("home-needs-attention");
      const rows = tile.getByRole("listitem");
      const shownTimes = () =>
        rows
          .locator("time")
          .evaluateAll((times) =>
            times.map((time) => time.getAttribute("datetime") ?? ""),
          );
      /** The tile shows exactly what it fetched, in the given order. */
      const expectTileShows = async (order: Order, kind: Kind) => {
        await expect(async () => {
          const fetched = fetchedTimes(order, kind);
          expect(fetched.length).toBeGreaterThan(0);
          expect(await shownTimes()).toEqual(fetched);
          expect(inOrder(fetched, order), fetched.join(", ")).toBe(true);
        }).toPass({ timeout: 15_000 });
      };
      const control = tile.getByRole("combobox", {
        name: "Sort attention items",
      });

      // Newest by default, with the same pill as the Workspaces sort.
      await expect(control).toHaveValue("newest", { timeout: 15_000 });
      await expect(control.locator("option")).toHaveText(["Newest", "Oldest"]);
      const workspacesSort = page.getByRole("combobox", {
        name: "Sort workspaces",
      });
      expect(await control.getAttribute("class")).toBe(
        await workspacesSort.getAttribute("class"),
      );
      await expectTileShows("newest", null);
      expect(page.url()).not.toContain("attentionSort");

      // Oldest reads the first page of the whole list from the server.
      responses = [];
      const oldestRequest = page.waitForRequest(
        (request) =>
          request.url().includes("/home/attention") &&
          new URL(request.url()).searchParams.get("order") === "oldest" &&
          !new URL(request.url()).searchParams.has("cursor"),
      );
      await control.selectOption("oldest");
      await oldestRequest;
      await expect(page).toHaveURL(/[?&]attentionSort=oldest(&|$)/);
      await expectTileShows("oldest", null);
      const firstPage = await rows.count();

      // Show more continues the same order from the server.
      const showMore = tile.getByTestId("home-attention-show-more");
      if ((await showMore.count()) > 0) {
        const nextRequest = page.waitForRequest(
          (request) =>
            request.url().includes("/home/attention") &&
            new URL(request.url()).searchParams.get("order") === "oldest" &&
            new URL(request.url()).searchParams.has("cursor"),
        );
        await showMore.click();
        await nextRequest;
        await expect
          .poll(() => rows.count(), { timeout: 15_000 })
          .toBeGreaterThan(firstPage);
        await expectTileShows("oldest", null);
      }

      // A reload keeps the order and starts at the first page.
      responses = [];
      await page.reload({ waitUntil: "domcontentloaded" });
      await expect(control).toHaveValue("oldest", { timeout: 15_000 });
      await expectTileShows("oldest", null);
      expect(responses.filter((response) => response.cursor)).toEqual([]);

      // The kind filter keeps the order.
      responses = [];
      await tile.getByTestId("home-attention-filter-approval").click();
      await expect(control).toHaveValue("oldest");
      await expectTileShows("oldest", "approval");

      // Back to Newest: the default leaves the URL.
      responses = [];
      await control.selectOption("newest");
      await expect(page).not.toHaveURL(/attentionSort/);
      await expectTileShows("newest", "approval");
    } finally {
      page.off("response", record);
      await authedApi(
        page,
        `/api/v1/workspaces/${targetId}/approvals/requirements/${definitionId}/retire`,
        "POST",
        {},
      ).catch(() => {});
    }
  });

  // Workbench Phase 7 (g5.workbench.tabs): under the alignment layout open
  // workspaces are tabs beside a pinned Home; "+" opens the organization
  // fork picker, whose fork lands as the new tab; the list survives a reload
  // per browser and closing the active tab falls back to its left neighbour.
  // workspaces g5.workspace.ui.chat-corner-button / platform_reports
  // g5.quality.platform-reports.trigger: the bottom-right button can be
  // dragged out of the way, opens Chat as the floating panel at the right edge
  // and hides while it is open or while Chat holds the center; the floating
  // report trigger is absent while the strip carries the chrome one.
  test("the corner Chat button moves, opens Chat at the right edge and Dock returns Chat to the center", async ({
    page,
  }) => {
    await openWorkspacePath(page, isolatedWorkspace.path);
    await expect(page.getByTestId("workspace-tab-strip")).toBeVisible({
      timeout: 15_000,
    });
    // The floating report trigger is never on a workspace page without an
    // overlay: with reporting advertised the strip carries the trigger,
    // without it there is none at all (the E2E stack does not configure
    // delivery, so reporting is normally off here).
    const info = await page.request.get("/api/v1/instance/info");
    const features: string[] = ((await info.json()).data ?? {}).features ?? [];
    await expect(
      page.getByTestId("platform-bug-report-topbar-button"),
    ).toHaveCount(features.includes("platform_issue_reporting") ? 1 : 0);
    await expect(page.getByTestId("platform-bug-report-button")).toHaveCount(0);

    const corner = page.getByTestId("chat-corner-button");
    await expect(corner).toBeVisible();

    // A drag moves the button and does not open Chat.
    const home = (await corner.boundingBox())!;
    const grip = { x: home.x + home.width / 2, y: home.y + home.height / 2 };
    await page.mouse.move(grip.x, grip.y);
    await page.mouse.down();
    await page.mouse.move(grip.x - 200, grip.y - 120, { steps: 10 });
    await page.mouse.up();
    const moved = (await corner.boundingBox())!;
    expect(Math.round(moved.x - home.x)).toBe(-200);
    expect(Math.round(moved.y - home.y)).toBe(-120);
    await expect(page.getByTestId("floating-panel-dock")).toHaveCount(0);

    await corner.click();
    const dock = page.getByTestId("floating-panel-dock");
    await expect(dock).toBeVisible();
    await expect(corner).toHaveCount(0);
    const panel = dock.locator("xpath=../..");
    const box = (await panel.boundingBox())!;
    const viewport = page.viewportSize()!;
    expect(Math.round(box.x + box.width)).toBe(viewport.width);
    expect(Math.round(box.y + box.height)).toBe(viewport.height);
    // Below the workbench header, so the ribbon's controls stay reachable.
    const header = (await page.getByTestId("workbench-header").boundingBox())!;
    expect(box.y).toBeGreaterThanOrEqual(Math.floor(header.y + header.height));
    await expect(
      page
        .getByTestId("action-ribbon")
        .getByTestId("workbench-workspace-controls")
        .getByTestId("workspace-info-button"),
    ).toBeVisible();
    await expect(page.getByTestId("center-chat-tab")).toHaveAttribute(
      "aria-selected",
      "false",
    );

    await dock.click();
    await expect(page.getByTestId("center-chat-tab")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // Chat is the center surface again, where the button has no job.
    await expect(page.getByTestId("chat-corner-button")).toHaveCount(0);
  });

  test("a long owner name truncates on its own line and never grows the ribbon", async ({
    page,
  }) => {
    await openWorkspacePath(page, isolatedWorkspace.path);
    const ribbon = page.getByTestId("action-ribbon");
    const owner = ribbon.getByTestId("workspace-owner-trigger");
    // The served configuration settles the ribbon's height (a fallback
    // banner would add to it): measure only once it has loaded, both times.
    const settled = async () => {
      await expect(ribbon).toHaveAttribute(
        "data-config-status",
        /builtin|committed/,
        { timeout: 15_000 },
      );
      await expect(owner).toContainText("Owner:");
    };
    await settled();
    const ribbonHeight = (await ribbon.boundingBox())!.height;
    const ownerHeight = (await owner.boundingBox())!.height;

    // The org's members, served with a display name far wider than a ribbon
    // button for everyone (so for the owner, the E2E user).
    const longName = "Maximilian Alexander von Habsburg-Lothringen";
    await page.route("**/api/v1/orgs/*/members", async (route) => {
      const response = await route.fetch();
      const body = (await response.json()) as
        | { displayName: string }[]
        | { data: { displayName: string }[] };
      const members = Array.isArray(body) ? body : body.data;
      for (const member of members) member.displayName = longName;
      await route.fulfill({ response, json: body });
    });
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await settled();
    await expect(owner).toContainText(longName);
    await expect(owner).toHaveAttribute("title", new RegExp(longName));
    // "Owner:" over the name on one truncating line: the ribbon and the
    // button keep their height, the name overflows its line with an
    // ellipsis, and the transfer caret stays visible beside it.
    await expect
      .poll(async () => (await ribbon.boundingBox())!.height)
      .toBe(ribbonHeight);
    await expect
      .poll(async () => (await owner.boundingBox())!.height)
      .toBe(ownerHeight);
    const name = owner.getByText(longName);
    expect(await name.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(
      true,
    );
    await expect(name).toHaveCSS("text-overflow", "ellipsis");
    await expect(owner.locator("svg.lucide-chevron-down")).toBeVisible();
  });

  test.describe(() => {
    test.describe.configure({ timeout: 420_000 });
    test("open workspaces are tabs beside a pinned Home and + forks into a new tab", async ({
      page,
    }) => {
      // The beforeEach clone is not started; run it so the ribbon shows Stop.
      await runWorkspaceVmAction(page, isolatedWorkspace.id, "resume");
      await waitForWorkspaceVmStatus(page, isolatedWorkspace.id, "running");
      await openWorkspacePath(page, isolatedWorkspace.path);

      const strip = page.getByTestId("workspace-tab-strip");
      await expect(strip).toBeVisible({ timeout: 15_000 });
      const cloneTab = strip.getByRole("tab", { name: isolatedWorkspace.name });
      await expect(cloneTab).toHaveAttribute("aria-selected", "true");
      await expect(strip.getByTestId("workspace-tab-home")).toHaveAttribute(
        "aria-selected",
        "false",
      );
      // The strip owns Home and there is no title bar on a desktop
      // (ui g5.ui.workbench-shell): the strip's right end carries the global
      // controls and the ribbon's right end the workspace controls, in the
      // ribbon's own button shape.
      await expect(page.locator('[data-surface="titlebar"]')).toHaveCount(0);
      await expect(page.getByTestId("workspace-vm-status-strip")).toHaveCount(
        0,
      );
      const shell = page
        .getByTestId("workspace-tab-bar")
        .getByTestId("shell-controls");
      await expect(shell.getByTestId("settings-button")).toBeVisible();
      await expect(shell.getByTestId("org-management-button")).toBeVisible();
      await expect(shell.getByTestId("top-bar-notifications")).toBeVisible();
      const controls = page
        .getByTestId("action-ribbon")
        .getByTestId("workbench-workspace-controls");
      await expect(
        controls.getByTestId("workspace-owner-trigger"),
      ).toContainText("Owner:");
      await expect(controls.getByTestId("workspace-info-button")).toContainText(
        "Details",
      );
      // Details carries the source download the toolbar used to offer.
      await controls.getByTestId("workspace-info-button").click();
      await expect(page.getByTestId("workspace-details-panel")).toBeVisible();
      await expect(page.getByTestId("workspace-download-source")).toBeVisible();
      // The running clone shows one Stop button, no pill.
      await expect(controls.getByTestId("workspace-vm-stop")).toContainText(
        "Stop",
      );
      await expect(controls.getByTestId("workspace-vm-start")).toHaveCount(0);
      await expect(controls.getByTestId("workspace-terminal-open")).toHaveCount(
        0,
      );
      await expect(controls.getByTestId("workspace-owner-trigger")).toHaveClass(
        new RegExp(
          (await page
            .getByTestId("ribbon-action-new-doc")
            .getAttribute("class"))!.split(" ")[0],
        ),
      );

      // Home is pinned; the workspace tab stays open.
      await strip.getByTestId("workspace-tab-home").click();
      await expect(page).toHaveURL(new RegExp(`/${TEST_ORG}/home$`));
      await expect(strip.getByTestId("workspace-tab-home")).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await expect(cloneTab).toHaveAttribute("aria-selected", "false");

      // "+" opens the organization's fork picker as a transient New tab.
      await strip.getByTestId("workspace-tab-add").click();
      await expect(page).toHaveURL(new RegExp(`/${TEST_ORG}/fork$`));
      await expect(strip.getByTestId("workspace-tab-new")).toHaveAttribute(
        "aria-selected",
        "true",
      );
      const picker = page.getByTestId("fork-picker");
      await expect(picker).toBeVisible({ timeout: 15_000 });
      // The picker has no title bar of its own; the strip's controls stay.
      await expect(page.locator('[data-surface="titlebar"]')).toHaveCount(0);
      await expect(page.getByTestId("shell-controls")).toBeVisible();
      const orgCard = picker.locator(
        `[data-testid="fork-picker-org"][data-org-slug="${TEST_ORG}"]`,
      );
      await expect(orgCard).toBeVisible();
      const baseRow = orgCard.locator(
        '[data-testid="fork-picker-row"][data-base="true"]',
      );
      await expect(baseRow).toBeVisible({ timeout: 15_000 });
      // The search narrows the tree and keeps a match's ancestors; clearing
      // it restores the rows.
      await page.getByTestId("fork-picker-search").fill(isolatedWorkspace.name);
      await expect(
        orgCard.locator(
          `[data-testid="fork-picker-row"][data-workspace-name="${isolatedWorkspace.name}"]`,
        ),
      ).toBeVisible();
      await expect(baseRow).toBeVisible();
      await page.getByTestId("fork-picker-search-clear").click();
      await expect(page.getByTestId("fork-picker-search")).toHaveValue("");
      await baseRow.getByTestId("fork-picker-fork").click();

      const dialog = page.getByRole("dialog");
      await expect(dialog).toBeVisible({ timeout: 15_000 });
      const forkName = workspaceCloneName("tabshell");
      await dialog.locator("#clone-name").fill(forkName);
      await dialog.getByTestId("clone-submit-button").click();
      try {
        // The fork lands on its workspace, which opens its tab in place of the
        // New tab.
        await expect(page).toHaveURL(new RegExp(`/${TEST_ORG}/${forkName}`), {
          timeout: 60_000,
        });
        await waitForWorkspaceLoad(page);
        const forkTab = strip.getByRole("tab", { name: forkName });
        await expect(forkTab).toHaveAttribute("aria-selected", "true");
        await expect(strip.getByTestId("workspace-tab-new")).toHaveCount(0);
        await expect(strip.getByTestId("workspace-tab")).toHaveCount(2);

        // Tabs survive a reload for this person in this browser.
        await page.reload({ waitUntil: "domcontentloaded" });
        await waitForWorkspaceLoad(page);
        await expect(strip.getByTestId("workspace-tab")).toHaveCount(2);
        await expect(
          strip.getByRole("tab", { name: forkName }),
        ).toHaveAttribute("aria-selected", "true");

        // Closing the active tab shows the tab on its left.
        await strip.getByRole("button", { name: `Close ${forkName}` }).click();
        await expect(page).toHaveURL(
          new RegExp(`/${TEST_ORG}/${isolatedWorkspace.name}`),
        );
        await expect(strip.getByTestId("workspace-tab")).toHaveCount(1);
        await expect(cloneTab).toHaveAttribute("aria-selected", "true");

        // A workspace's name in the picker opens it as the new tab (the
        // locators above resolve against the reopened picker).
        await strip.getByTestId("workspace-tab-add").click();
        await expect(baseRow).toBeVisible({ timeout: 15_000 });
        const baseName = await baseRow.getAttribute("data-workspace-name");
        await baseRow.getByTestId("fork-picker-open").click();
        await expect(page).toHaveURL(new RegExp(`/${TEST_ORG}/${baseName}$`));
        await waitForWorkspaceLoad(page);
        await expect(strip.getByTestId("workspace-tab-new")).toHaveCount(0);
        await expect(strip.getByTestId("workspace-tab")).toHaveCount(2);
        await expect(
          strip.getByRole("tab", { name: baseName! }),
        ).toHaveAttribute("aria-selected", "true");
      } finally {
        const forkId = await resolveWorkspaceId(page, TEST_ORG, forkName).catch(
          () => "",
        );
        await deleteWorkspaceById(page, forkId).catch(() => {});
      }
    });
  });

  test("workspace tabs reorder by dragging beside a fixed Home and keep their order", async ({
    page,
  }) => {
    // ui g5.workbench.tabs.reorder. The stored list is per person and
    // browser; seed it with the clone, the test workspace, unlisted fillers
    // that make the strip overflow and another organization's tab.
    // Arriving at the clone wrote this person's list; its key names them.
    let key = "";
    await expect
      .poll(async () => {
        key = await page.evaluate(
          () =>
            Object.keys(localStorage).find((name) =>
              name.startsWith("g5:workspaceTabs:"),
            ) ?? "",
        );
        return key;
      })
      .not.toBe("");
    // Enough tabs to overflow a desktop strip; a narrower window switches to
    // the mobile layout, which has no strip.
    const fillers = Array.from({ length: 10 }, (_, i) => `zz-tab-filler-${i}`);
    const otherOrg = { orgSlug: "zz-other-org", name: "kept" };
    const clone = isolatedWorkspace.name;
    await page.evaluate(
      ({ key, tabs }) => localStorage.setItem(key, JSON.stringify(tabs)),
      {
        key,
        tabs: [
          otherOrg,
          ...[clone, ...fillers, TEST_WORKSPACE].map((name) => ({
            orgSlug: TEST_ORG,
            name,
          })),
        ],
      },
    );
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);

    const strip = page.getByTestId("workspace-tab-strip");
    const scroller = page.getByTestId("workspace-tab-scroller");
    const home = strip.getByTestId("workspace-tab-home");
    const tab = (name: string) => strip.getByRole("tab", { name, exact: true });
    const order = () =>
      strip
        .getByTestId("workspace-tab")
        .evaluateAll((els) => els.map((el) => el.getAttribute("aria-label")));
    const stored = () =>
      page.evaluate(
        (key) =>
          (
            JSON.parse(localStorage.getItem(key) ?? "[]") as {
              orgSlug: string;
              name: string;
            }[]
          ).map((entry) => `${entry.orgSlug}/${entry.name}`),
        key,
      );
    const scrollState = () =>
      scroller.evaluate((el) => ({
        left: Math.round(el.scrollLeft),
        max: el.scrollWidth - el.clientWidth,
      }));
    const expectCloneSelected = async () => {
      await expect(tab(clone)).toHaveAttribute("aria-selected", "true");
      await expect(page).toHaveURL(
        new RegExp(`/${TEST_ORG}/${clone}(?:[/?#]|$)`),
      );
    };
    /** Press a tab, drag it to `x` and keep holding until `settled`. */
    const dragTo = async (
      name: string,
      x: number,
      settled: () => Promise<boolean>,
    ) => {
      await tab(name).scrollIntoViewIfNeeded();
      const from = (await tab(name).boundingBox())!;
      const y = from.y + from.height / 2;
      await page.mouse.move(from.x + 12, y);
      await page.mouse.down();
      await page.mouse.move(from.x + 24, y, { steps: 3 });
      await page.mouse.move(x, y, { steps: 12 });
      await expect.poll(settled, { timeout: 10_000 }).toBe(true);
    };

    await expect.poll(order).toEqual([clone, ...fillers, TEST_WORKSPACE]);
    await expectCloneSelected();
    await expect.poll(async () => (await scrollState()).max).toBeGreaterThan(0);

    // The last tab, dragged to the left edge, scrolls the strip back and
    // lands first, right after Home; the active tab stays selected.
    const box = (await scroller.boundingBox())!;
    await dragTo(
      TEST_WORKSPACE,
      box.x + 4,
      async () => (await scrollState()).left === 0,
    );
    await page.mouse.up();
    await expect.poll(order).toEqual([TEST_WORKSPACE, clone, ...fillers]);
    await expectCloneSelected();
    expect(await stored()).toEqual([
      `${otherOrg.orgSlug}/${otherOrg.name}`,
      ...[TEST_WORKSPACE, clone, ...fillers].map((n) => `${TEST_ORG}/${n}`),
    ]);

    // The active tab, held at the right edge, scrolls the overflowing strip
    // to its end and lands last. Home stays at the left throughout.
    await dragTo(clone, box.x + box.width - 4, async () => {
      const state = await scrollState();
      return state.left >= state.max - 1;
    });
    await expect(home).toBeInViewport();
    expect((await home.boundingBox())!.x).toBeLessThan(box.x);
    await page.mouse.up();
    await expect.poll(order).toEqual([TEST_WORKSPACE, ...fillers, clone]);
    await expectCloneSelected();

    // Escape cancels a drag: nothing moves and nothing is selected.
    const filler = (await tab(fillers[fillers.length - 1]).boundingBox())!;
    await page.mouse.move(filler.x + 12, filler.y + filler.height / 2);
    await page.mouse.down();
    await page.mouse.move(filler.x - 200, filler.y + filler.height / 2, {
      steps: 10,
    });
    await page.keyboard.press("Escape");
    await page.mouse.up();
    await expect.poll(order).toEqual([TEST_WORKSPACE, ...fillers, clone]);
    await expectCloneSelected();

    // The order survives a reload for this person in this browser.
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await expect.poll(order).toEqual([TEST_WORKSPACE, ...fillers, clone]);
    await expectCloneSelected();

    // The keyboard moves the focused tab too; closing the active tab then
    // shows its new left neighbour.
    await tab(clone).focus();
    for (let i = 0; i < fillers.length; i += 1) {
      await page.keyboard.press("Control+Shift+ArrowLeft");
    }
    await expect.poll(order).toEqual([TEST_WORKSPACE, clone, ...fillers]);
    await expect(tab(clone)).toBeFocused();
    await strip.getByRole("button", { name: `Close ${clone}` }).click();
    await expect(page).toHaveURL(
      new RegExp(`/${TEST_ORG}/${TEST_WORKSPACE}(?:[/?#]|$)`),
    );
    await expect(tab(TEST_WORKSPACE)).toHaveAttribute("aria-selected", "true");
    await expect.poll(order).toEqual([TEST_WORKSPACE, ...fillers]);
    expect(await stored()).toContain(`${otherOrg.orgSlug}/${otherOrg.name}`);
  });
});
