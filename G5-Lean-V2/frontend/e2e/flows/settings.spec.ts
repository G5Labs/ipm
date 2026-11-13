import { test, expect, type Page, type Locator } from "@playwright/test";
import { THEME_OPTIONS } from "../../src/lib/defaults";
import {
  navigateToWorkspace,
  getCurrentTheme,
  getCurrentMode,
  TEST_ORG,
  TEST_WORKSPACE,
} from "../helpers";

// Theme/editor/model/SCM settings render inside the workspace chrome as a
// center-panel tab, opened from the consolidated top-right settings control.
// The `workspace-settings-page` testid is preserved on the tab content. The
// legacy `/:org/settings` and `/settings` routes redirect into the org's base
// workspace (`?view=settings`), which opens the same tab.
async function openWorkspaceSettings(page: Page): Promise<Locator> {
  await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);

  await page.getByTestId("settings-button").click();

  const settingsPage = page.getByTestId("workspace-settings-page");
  await expect(settingsPage).toBeVisible({ timeout: 10_000 });

  await expect(settingsPage.getByTestId("theme-selector")).toBeVisible({
    timeout: 10_000,
  });
  await expect(settingsPage.getByTestId("mode-toggle")).toBeVisible({
    timeout: 10_000,
  });

  return settingsPage;
}

test.describe("Settings", () => {
  test("legacy /settings redirects to the workspace settings page", async ({
    page,
  }) => {
    await page.goto("/settings");
    await expect(page.getByTestId("workspace-settings-page")).toBeVisible({
      timeout: 10_000,
    });
  });

  test("change theme and verify CSS update", async ({ page }) => {
    const settingsPage = await openWorkspaceSettings(page);
    const initialTheme = await getCurrentTheme(page);

    const themeButtons = settingsPage
      .getByTestId("theme-selector")
      .getByRole("button");
    const count = await themeButtons.count();
    let changedTheme = false;

    for (let i = 0; i < count; i++) {
      const button = themeButtons.nth(i);
      const label = await button.getAttribute("aria-label");
      if (label && !label.toLowerCase().includes(initialTheme.toLowerCase())) {
        await button.click();
        changedTheme = true;
        break;
      }
    }

    expect(changedTheme).toBe(true);
    await expect.poll(() => getCurrentTheme(page)).not.toBe(initialTheme);
  });

  test("aero glass theme paints glass chrome and keeps the mode toggle working", async ({
    page,
  }) => {
    const settingsPage = await openWorkspaceSettings(page);
    const initialTheme = await getCurrentTheme(page);
    const initialMode = await getCurrentMode(page);

    await settingsPage
      .getByRole("button", { name: "Select Aero Glass theme" })
      .click();
    await expect.poll(() => getCurrentTheme(page)).toBe("aero-glass");

    // The tab strip is the shell's chrome row (no desktop title bar).
    const titleBar = page.locator('[data-surface="chrome"]').first();
    await expect(titleBar).toBeVisible();
    await expect
      .poll(() =>
        titleBar.evaluate((el) => getComputedStyle(el).backgroundImage),
      )
      .toContain("gradient");

    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();
    await expect.poll(() => getCurrentMode(page)).not.toBe(initialMode);
    await expect
      .poll(() =>
        titleBar.evaluate((el) => getComputedStyle(el).backgroundImage),
      )
      .toContain("gradient");

    // Restore the persisted settings so later tests start from the same state.
    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();
    await expect.poll(() => getCurrentMode(page)).toBe(initialMode);
    const initialLabel = THEME_OPTIONS.find(
      (t) => t.name === initialTheme,
    )?.label;
    expect(initialLabel, `theme ${initialTheme} is registered`).toBeDefined();
    await settingsPage
      .getByTestId("theme-selector")
      .getByRole("button", { name: `Select ${initialLabel} theme` })
      .click();
    await expect.poll(() => getCurrentTheme(page)).toBe(initialTheme);
  });

  test("classic 98 theme bevels the shell and keeps the mode toggle working", async ({
    page,
  }) => {
    const settingsPage = await openWorkspaceSettings(page);
    const initialTheme = await getCurrentTheme(page);
    const initialMode = await getCurrentMode(page);

    await settingsPage
      .getByRole("button", { name: "Select Old school theme" })
      .click();
    await expect.poll(() => getCurrentTheme(page)).toBe("classic-98");

    // The skin draws its window bevel with inset shadows on the shell and
    // squares every corner; both must hold in each mode.
    const shell = page.locator('[data-surface="shell"]').first();
    await expect(shell).toBeVisible();
    const bevelled = () =>
      shell.evaluate((el) => {
        const cs = getComputedStyle(el);
        return cs.boxShadow.includes("inset") && cs.borderRadius === "0px";
      });
    await expect.poll(bevelled).toBe(true);

    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();
    await expect.poll(() => getCurrentMode(page)).not.toBe(initialMode);
    await expect.poll(bevelled).toBe(true);

    // Restore the persisted settings so later tests start from the same state.
    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();
    await expect.poll(() => getCurrentMode(page)).toBe(initialMode);
    const initialLabel = THEME_OPTIONS.find(
      (t) => t.name === initialTheme,
    )?.label;
    expect(initialLabel, `theme ${initialTheme} is registered`).toBeDefined();
    await settingsPage
      .getByTestId("theme-selector")
      .getByRole("button", { name: `Select ${initialLabel} theme` })
      .click();
    await expect.poll(() => getCurrentTheme(page)).toBe(initialTheme);
  });

  test("green room theme paints the black rail and keeps the mode toggle working", async ({
    page,
  }) => {
    const settingsPage = await openWorkspaceSettings(page);
    const initialTheme = await getCurrentTheme(page);
    const initialMode = await getCurrentMode(page);

    await settingsPage
      .getByRole("button", { name: "Select Green room theme" })
      .click();
    await expect.poll(() => getCurrentTheme(page)).toBe("green-room");

    // The skin paints the sidebar in its rail tone and rounds the tab strip's
    // active tab into a pill; both must hold in each mode.
    const sidebar = page.locator('[data-surface="sidebar"]').first();
    await expect(sidebar).toBeVisible();
    const railTone = () =>
      sidebar.evaluate((el) => getComputedStyle(el).backgroundColor);
    await expect.poll(railTone).toBe("rgb(0, 0, 0)");

    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();
    await expect.poll(() => getCurrentMode(page)).not.toBe(initialMode);
    await expect.poll(railTone).toBe("rgb(240, 240, 240)");

    // Restore the persisted settings so later tests start from the same state.
    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();
    await expect.poll(() => getCurrentMode(page)).toBe(initialMode);
    const initialLabel = THEME_OPTIONS.find(
      (t) => t.name === initialTheme,
    )?.label;
    expect(initialLabel, `theme ${initialTheme} is registered`).toBeDefined();
    await settingsPage
      .getByTestId("theme-selector")
      .getByRole("button", { name: `Select ${initialLabel} theme` })
      .click();
    await expect.poll(() => getCurrentTheme(page)).toBe(initialTheme);
  });

  test("toggle dark/light mode", async ({ page }) => {
    const settingsPage = await openWorkspaceSettings(page);
    const initialMode = await getCurrentMode(page);

    await settingsPage
      .getByTestId("mode-toggle")
      .getByRole("switch", { name: /toggle dark\/light mode/i })
      .click();

    await expect.poll(() => getCurrentMode(page)).not.toBe(initialMode);
  });
});
