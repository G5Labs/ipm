import { expect, test } from "@playwright/test";
import { openWorkspacePath, TEST_ORG, TEST_WORKSPACE } from "../helpers";

test.describe("Issue 2394 — Code Uplift Sync removal", () => {
  test("E09 browser has no creation affordance for the retired shape", async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== "chromium", "Removal UI matrix runs once.");
    await openWorkspacePath(page, `/${TEST_ORG}/${TEST_WORKSPACE}`);

    await expect(
      page.getByRole("button", { name: /code uplift sync/i }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("menuitem", { name: /code uplift sync/i }),
    ).toHaveCount(0);
  });
});
