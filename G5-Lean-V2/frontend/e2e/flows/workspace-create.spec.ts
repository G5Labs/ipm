import { expect, test, type Page } from "@playwright/test";

import { authedApi, TEST_SELECTED_ORG } from "../helpers";

async function selectTargetOrganization(page: Page) {
  const me = await authedApi(page, "/api/v1/users/me");
  expect(me.status, JSON.stringify(me.body)).toBe(200);
  const data = me.body?.data ?? me.body ?? {};
  const orgs = Array.isArray(data.orgs) ? data.orgs : [];
  const targetIndex = orgs.findIndex(
    (org: Record<string, unknown>) =>
      (org.orgSlug ?? org.org_slug ?? org.slug) === TEST_SELECTED_ORG,
  );
  expect(targetIndex).toBeGreaterThanOrEqual(0);
  if (orgs.length > 1) {
    await page
      .getByRole("combobox", {
        name: /select organization for new project/i,
      })
      .selectOption(String(targetIndex));
  }
}

async function openCreateDialog(page: Page) {
  await page.goto("/projects", { waitUntil: "domcontentloaded" });
  await expect(
    page.getByRole("button", { name: /new project/i }),
  ).toBeVisible({ timeout: 30_000 });
  await selectTargetOrganization(page);
  await page.getByRole("button", { name: /new project/i }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: 15_000 });
  return dialog;
}

test.describe("Create workspace through the base fork service", () => {
  test("shows the same exact-name member mapping as Fork", async ({ page }) => {
    const dialog = await openCreateDialog(page);
    await expect(dialog.locator("#clone-dialog-title")).toContainText(/fork/i);
    await expect(dialog.locator("#clone-name")).toBeFocused();
    await expect(dialog.getByTestId("clone-submit-button")).toBeDisabled();

    const name = `create-shared-${Date.now()}`;
    await dialog.locator("#clone-name").fill(name);
    // The per-repository branch names live in the Advanced section.
    await dialog.getByTestId("clone-advanced-toggle").click();
    const rows = dialog.getByTestId("clone-member-mapping-row");
    expect(await rows.count()).toBeGreaterThan(0);
    for (let index = 0; index < (await rows.count()); index += 1) {
      await expect(rows.nth(index)).toContainText(name);
    }
    await expect(
      dialog.getByRole("button", { name: /^fork$/i }),
    ).toBeEnabled();
    await expect(dialog.getByRole("button", { name: /add repo/i })).toHaveCount(
      0,
    );
  });

  test("posts to clone and never calls removed create or add-repo endpoints", async ({
    page,
  }) => {
    const removedCalls: string[] = [];
    page.on("request", (request) => {
      const path = new URL(request.url()).pathname;
      if (
        request.method() === "POST" &&
        (/\/api\/v1\/orgs\/[^/]+\/workspaces(?:\/init)?$/.test(path) ||
          /\/api\/v1\/workspaces\/[^/]+\/repos$/.test(path))
      ) {
        removedCalls.push(path);
      }
    });

    const dialog = await openCreateDialog(page);
    const name = `create-via-fork-${Date.now()}`;
    await dialog.locator("#clone-name").fill(name);
    const cloneRequest = page.waitForRequest(
      (request) =>
        /\/api\/v1\/workspaces\/[^/]+\/clone$/.test(
          new URL(request.url()).pathname,
        ) && request.method() === "POST",
    );
    await dialog.getByTestId("clone-submit-button").click();
    const request = await cloneRequest;
    expect(request.postDataJSON()?.name).toBe(name);
    expect(request.postDataJSON()?.branches).toBeUndefined();
    expect(removedCalls).toEqual([]);
  });

  test("can be cancelled without a request", async ({ page }) => {
    const dialog = await openCreateDialog(page);
    await dialog.getByTestId("clone-cancel-button").click();
    await expect(dialog).not.toBeVisible();
  });
});
