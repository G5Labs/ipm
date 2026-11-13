import { test, expect, type Page } from "@playwright/test";
import {
  GUIDED_JOURNEY_ENABLED,
  authedApi,
  createWorkspaceClone,
  deleteWorkspaceById,
  TEST_ORG,
  TEST_WORKSPACE,
  type WorkspaceClone,
  patchStoredUserSettings,
  showWorkspaceTabs,
  waitForWorkspaceLoad,
} from "../helpers";

// Guided journey (ui g5.ui.guided-panel, g5.ui.journey-evidence,
// g5.ui.guided-toggle, g5.ui.guided-collapsed). The gate comes from the
// shared helpers: `guided_journey` default-on.

/** Guided starts collapsed like every section (g5.ui.guided-collapsed): expand it. */
async function openGuided(page: Page): Promise<void> {
  const section = page.getByTestId("sidebar-guided-tab");
  await expect(section).toBeVisible({ timeout: 15_000 });
  const header = section.getByRole("button", { name: /guided/i }).first();
  if ((await header.getAttribute("aria-expanded")) !== "true") {
    await header.click();
  }
  await expect(page.getByTestId("guided-journey")).toBeVisible({
    timeout: 15_000,
  });
}

test.describe("Guided journey", () => {
  test.skip(
    !GUIDED_JOURNEY_ENABLED,
    "Guided journey needs G5_GUIDED_JOURNEY_ENABLED not set to false",
  );

  let isolatedWorkspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    const start = testInfo.title.startsWith("a mock document");
    await showWorkspaceTabs(page);
    isolatedWorkspace = await createWorkspaceClone(page, {
      org: TEST_ORG,
      workspace: TEST_WORKSPACE,
      namePrefix: `guided-${testInfo.title}`,
      // Project Docs live in the workspace's docs checkout, so seeding the
      // outcome documents needs a running workspace.
      start,
    });
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, isolatedWorkspace?.id).catch(() => {});
  });

  test("starts collapsed on a fork like every section, lists the active phase's steps with their buttons once expanded, and starts collapsed again after a reload", async ({
    page,
  }) => {
    const section = page.getByTestId("sidebar-guided-tab");
    await expect(section).toBeVisible({ timeout: 15_000 });
    const journey = page.getByTestId("guided-journey");
    // Collapsed on open and fork (g5.ui.guided-collapsed).
    await expect(journey).toBeHidden();
    await section
      .getByRole("button", { name: /guided/i })
      .first()
      .click();
    await expect(journey).toBeVisible();
    // A fork of the test workspace starts in Business Engineering: the next
    // BE step is open with its buttons, the others open on their label
    // (g5.ui.guided-panel).
    await expect(page.getByTestId("guided-phase-be")).toHaveAttribute(
      "data-relation",
      "active",
    );
    await expect(
      page.getByTestId("guided-step-be-intent-action-new-doc"),
    ).toBeVisible();
    await page.getByTestId("guided-step-be-mock-toggle").click();
    await expect(
      page.getByTestId("guided-step-be-mock-action-build-mock"),
    ).toBeVisible();
    await expect(page.getByTestId("guided-summary")).toContainText(
      "Business Engineering",
    );

    // Nothing is remembered: a reload starts collapsed again, as does an
    // expand left open.
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await expect(page.getByTestId("sidebar-guided-tab")).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByTestId("guided-journey")).toBeHidden();
  });

  test("in the expand-on-hover sidebar nothing opens on load; the Guided icon opens the overlay and leaving closes it", async ({
    page,
  }) => {
    await patchStoredUserSettings(page, {
      sidebarDisplayMode: "expand-on-hover",
    });
    await page.goto(isolatedWorkspace.path, { waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    const overlay = page.getByTestId("sidebar-rail-overlay");
    await expect(page.getByTestId("sidebar-rail-guided")).toBeVisible({
      timeout: 15_000,
    });
    await expect(overlay).toHaveCount(0);
    await expect(page.getByTestId("sidebar-rail-guided")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    // Hovering the Guided icon opens it expanded; leaving closes it.
    await page.getByTestId("sidebar-rail-guided").hover();
    await expect(overlay).toBeVisible();
    await expect(page.getByTestId("guided-journey")).toBeVisible();
    await page.mouse.move(900, 400);
    await expect(overlay).toHaveCount(0);
  });

  test("Mark done persists as the owner's durable mark, shows on the timeline, and clears with Not done", async ({
    page,
  }) => {
    await openGuided(page);
    // Move to System Engineering so the diff-review step (marks only) is in
    // the active phase.
    const workspaceId = isolatedWorkspace.id;
    const moved = await authedApi(
      page,
      `/api/v1/workspaces/${workspaceId}`,
      "PATCH",
      { phase: "se" },
    );
    expect(moved.status).toBe(200);
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await openGuided(page);

    const review = page.getByTestId("guided-step-se-diff-review");
    await expect(review).toHaveAttribute("data-state", "todo", {
      timeout: 15_000,
    });
    await page.getByTestId("guided-step-se-diff-review-mark").click();
    await expect(review).toHaveAttribute("data-state", "done");

    // Durable: the mark survives a reload and the API reports it.
    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForWorkspaceLoad(page);
    await openGuided(page);
    await expect(
      page.getByTestId("guided-step-se-diff-review"),
    ).toHaveAttribute("data-state", "done", { timeout: 15_000 });
    const marks = (await authedApi(
      page,
      `/api/v1/workspaces/${workspaceId}/journey`,
    )) as { body: { data: { marks: unknown[] } } };
    expect(marks.body.data.marks).toEqual([
      expect.objectContaining({
        stepId: "se-diff-review",
        done: true,
        skipped: false,
      }),
    ]);

    // The mark is a timeline entry under its own filter group; the ribbon's
    // Overview control opens the page (g5.workbench.landing).
    await page.getByTestId("ribbon-overview").click();
    await expect(
      page
        .getByTestId("workspace-activity-entries")
        .getByText("Marked done: Check what changed"),
    ).toBeVisible({ timeout: 15_000 });
    // Guided steps are part of the default All selection.
    await expect(page.getByTestId("activity-filter-all")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(
      page.getByTestId("activity-filter-guided_steps"),
    ).toBeVisible();

    await page.getByTestId("guided-step-se-diff-review-mark").click();
    await expect(
      page.getByTestId("guided-step-se-diff-review"),
    ).toHaveAttribute("data-state", "todo");
  });

  test.describe(() => {
    test.describe.configure({ timeout: 300_000 });
    test("a mock document completes its step, renders as a chip that opens the document, and the summary offers the phase move", async ({
      page,
    }) => {
      await openGuided(page);
      const workspaceId = isolatedWorkspace.id;
      // The outcome documents of Business Engineering, through the ordinary
      // Project Docs API: a requirement (intent) and a mock.
      for (const [name, filePath] of [
        ["requirements.md", `docs/${workspaceId}/requirements.md`],
        ["home.html", `docs/${workspaceId}/mocks/home.html`],
      ]) {
        const seeded = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs`,
          "POST",
          {
            name,
            docType: "design",
            filePath,
            content: name.endsWith(".html")
              ? "<!doctype html><title>Home</title><p>mock</p>"
              : "# Requirements\n\nOne screen.",
          },
        );
        expect(seeded.status, `${name}: ${JSON.stringify(seeded.body)}`).toBe(
          201,
        );
      }
      // The open-questions step is the owner's call here.
      const marked = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/journey/steps/be-open-questions`,
        "PUT",
        { done: true },
      );
      expect(marked.status).toBe(200);
      await page.reload({ waitUntil: "domcontentloaded" });
      await waitForWorkspaceLoad(page);
      await openGuided(page);

      const mock = page.getByTestId("guided-step-be-mock");
      await expect(mock).toHaveAttribute("data-state", "done", {
        timeout: 15_000,
      });
      await expect(page.getByTestId("guided-step-be-intent")).toHaveAttribute(
        "data-state",
        "done",
      );
      // The chip is the chat's citation chip and opens the document.
      const chip = page
        .getByTestId("guided-step-be-mock-artifacts")
        .getByTestId("chat-message-citation");
      // The listing names a Markdown-less HTML doc by its file stem.
      await expect(chip).toHaveText(/^home(\.html)?$/);
      await chip.click();
      await expect(
        page.getByTestId("center-content-tab").filter({ hasText: /home/ }),
      ).toBeVisible({ timeout: 15_000 });

      // Every BE step is done: the summary offers the owner's move, which is
      // the chevrons' move and persists.
      const move = page.getByTestId("guided-summary-move");
      await expect(move).toHaveText(/Move to System Engineering/);
      // The chevron updates optimistically. Reloading on that alone can
      // cancel the PATCH before it commits, so require the durable response.
      const phaseSaved = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname ===
            `/api/v1/workspaces/${workspaceId}` &&
          response.request().method() === "PATCH" &&
          response.request().postDataJSON()?.phase === "se",
      );
      await move.click();
      const saved = await phaseSaved;
      expect(saved.status(), await saved.text()).toBe(200);
      expect((await saved.json()).data.phase).toBe("se");
      await expect(page.getByTestId("phase-chevrons")).toHaveAttribute(
        "data-phase",
        "se",
        { timeout: 15_000 },
      );
      await page.reload({ waitUntil: "domcontentloaded" });
      await waitForWorkspaceLoad(page);
      await openGuided(page);
      await expect(page.getByTestId("phase-chevrons")).toHaveAttribute(
        "data-phase",
        "se",
        { timeout: 15_000 },
      );
      await expect(page.getByTestId("guided-phase-se")).toHaveAttribute(
        "data-relation",
        "active",
      );
    });
  });

  test("a phase without requirements links to the approvals tab on that phase", async ({
    page,
  }) => {
    await openGuided(page);
    await expect(page.getByTestId(/^guided-gate-/)).toHaveCount(0);
    await page.getByTestId("ribbon-workspace-approvals").click();
    await expect(
      page.getByRole("heading", { name: /^Approvals Required to merge into / }),
    ).toBeVisible();
    await expect(
      page.getByTestId("workspace-approvals-phase-filter"),
    ).toHaveCount(0);
  });
});
