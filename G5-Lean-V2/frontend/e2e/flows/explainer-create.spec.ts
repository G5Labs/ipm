// Create HTML Explainer entry point (plan `explainer-create-cutover` v1,
// left-sidebar D33/D34). Successor of the retired view-create-edit.spec.ts:
// persona-view creation is retired everywhere (`POST /docs/views` → 410 Gone).
// Since 2026-10-30 the rail has no Explainers section (explainers are Project
// Docs under `explainers/`); the ribbon's Explainer action in the System
// Engineering phase launches the Create Explainer modal. Owning system
// contract: `test_contract/view_creation_sync` I36/I37.

import { test, expect, type Locator, type Page } from "@playwright/test";
import {
  authedApi,
  captureChatTurn,
  releaseChatTurn,
  selectAgentProviderViaSettings,
  createWorkspaceClone,
  deleteWorkspaceById,
  navigateToWorkspace,
  TEST_ORG,
  type WorkspaceClone,
} from "../helpers";

/**
 * Open Create Explainer from the ribbon: Explainer is a System Engineering
 * action, so the owner's workspace is moved there first.
 */
async function openExplainerFromRibbon(page: Page): Promise<Locator> {
  const chevrons = page.getByTestId("phase-chevrons");
  await expect(chevrons).toBeVisible({ timeout: 30_000 });
  if ((await chevrons.getAttribute("data-phase")) !== "se") {
    await page.getByTestId("phase-chevron-se").click();
    await expect(chevrons).toHaveAttribute("data-phase", "se", {
      timeout: 15_000,
    });
  }
  const action = page.getByTestId("ribbon-action-explainer");
  await expect(action).toBeVisible({ timeout: 30_000 });
  await expect(action).toBeEnabled({ timeout: 30_000 });
  await action.click();
  const modal = page.getByRole("dialog", { name: "Create Explainer" });
  await expect(modal).toBeVisible({ timeout: 5_000 });
  return modal;
}

test.describe("Explainer Creation", () => {
  let isolatedWorkspace: WorkspaceClone | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    isolatedWorkspace = await createWorkspaceClone(page, {
      namePrefix: `explainer-create-${testInfo.title}`,
      start: true,
    });
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);
  });

  test.afterEach(async ({ page }) => {
    if (isolatedWorkspace) {
      await deleteWorkspaceById(page, isolatedWorkspace.id);
    }
  });

  test("the ribbon's Explainer action opens the Create Explainer modal", async ({
    page,
  }) => {
    // No Explainers rail section remains (owner instruction, 2026-10-30).
    await expect(page.getByTestId("sidebar-explainers-tab")).toHaveCount(0);
    await expect(page.getByTestId("sidebar-rail-explainers")).toHaveCount(0);
    const modal = await openExplainerFromRibbon(page);

    // Core modal surfaces remain available without the retired node picker.
    await expect(modal.getByTestId("create-explainer-topic")).toBeVisible();
    await expect(modal.getByTestId("create-explainer-node-query")).toHaveCount(
      0,
    );
    await expect(
      modal.getByTestId("create-explainer-whole-workspace"),
    ).toHaveCount(0);
    await expect(modal.getByTestId("create-explainer-prompt")).toBeVisible();
    await expect(
      modal.getByTestId("create-explainer-format-page"),
    ).toBeVisible();
    await expect(
      modal.getByTestId("create-explainer-format-slides"),
    ).toBeVisible();

    // Create View retirement (I37): no Create View modal, persona grid, or
    // entry point is reachable anywhere on the page.
    await expect(page.getByRole("dialog", { name: "Create View" })).toHaveCount(
      0,
    );
    await expect(
      page.getByRole("button", { name: "Create view", exact: true }),
    ).toHaveCount(0);

    await modal.getByRole("button", { name: "Close" }).click();
    await expect(modal).not.toBeVisible();

    // Reopening yields the same clean modal state.
    await openExplainerFromRibbon(page);
    await expect(modal.getByTestId("create-explainer-topic")).toHaveValue("");
    await expect(modal.getByTestId("create-explainer-node-query")).toHaveCount(
      0,
    );
    await modal.getByRole("button", { name: "Close" }).click();
  });
  test("submitting an explainer starts an ordinary agent conversation", async ({
    page,
  }) => {
    await selectAgentProviderViaSettings(page, "claude_code");
    const modal = await openExplainerFromRibbon(page);
    await modal
      .getByTestId("create-explainer-topic")
      .fill("Agent execution smoke check");
    await modal
      .getByTestId("create-explainer-template-select")
      .selectOption({ label: "Custom (write my own)" });
    await modal
      .getByTestId("create-explainer-prompt")
      .fill("Explain this workspace briefly.");
    const submit = modal.getByTestId("create-explainer-submit");
    await expect(submit).toBeEnabled({ timeout: 30_000 });
    const response = page.waitForResponse(
      (value) =>
        value.request().method() === "POST" &&
        new URL(value.url()).pathname ===
          `/api/v1/workspaces/${isolatedWorkspace!.id}/sessions/start`,
    );
    await submit.click();
    const turn = await captureChatTurn(page, await response);
    try {
      await expect(modal).not.toBeVisible();
      await expect(page.getByTestId("chat-input").first()).toBeVisible();
      const task = await authedApi(page, `/api/v1/tasks/${turn.taskId}`);
      expect(task.status, JSON.stringify(task.body)).toBe(200);
      expect(task.body.data.taskType).toBe("agent");
      expect(task.body.data.workspaceId).toBe(isolatedWorkspace!.id);
      expect(task.body.data.sessionId).toBe(turn.sessionId);
      // Delivery moves the message from the inbox to the transcript. Read
      // both boundaries so fast and slow provider startup are equally valid.
      await expect
        .poll(
          async () => {
            const inputs = await authedApi(
              page,
              `/api/v1/sessions/${turn.sessionId}/inputs`,
            );
            expect(inputs.status, JSON.stringify(inputs.body)).toBe(200);
            if (
              JSON.stringify(inputs.body).includes(
                "Agent execution smoke check",
              )
            ) {
              return true;
            }
            const messages = await authedApi(
              page,
              `/api/v1/sessions/${turn.sessionId}/messages`,
            );
            expect(messages.status, JSON.stringify(messages.body)).toBe(200);
            return JSON.stringify(messages.body).includes(
              "Agent execution smoke check",
            );
          },
          { timeout: 15_000 },
        )
        .toBe(true);
    } finally {
      await releaseChatTurn(page, turn);
    }
  });
});
