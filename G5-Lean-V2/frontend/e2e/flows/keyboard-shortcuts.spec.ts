import { test, expect, type Page } from '@playwright/test';
import {
  navigateToWorkspace,
  TEST_ORG,
  TEST_WORKSPACE,
} from '../helpers';

/** Press the platform's Cmd/Ctrl + key chord. */
async function pressMod(page: Page, key: string): Promise<void> {
  const isMac = await page.evaluate(() => navigator.platform.includes('Mac'));
  await page.keyboard.press(`${isMac ? 'Meta' : 'Control'}+${key}`);
}

test.describe('Keyboard Shortcuts', () => {
  test('no workspace search surface exists and Cmd+K opens none', async ({
    page,
  }) => {
    // P8.9 removed workspace search end to end. The docked top-bar input is
    // gone, and no shortcut may bring back a spotlight overlay, a results
    // panel or a modal. This is the browser-level half of the boundary that
    // workspace_search_removal_guard.rs covers at the API and schema level.
    await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);

    // The workspace shell is mounted — that is what makes the negative
    // assertions below meaningful rather than vacuous.
    await expect(page.getByTestId('center-panel')).toBeVisible({
      timeout: 30_000,
    });

    await expect(page.getByTestId('search-input-docked')).toHaveCount(0);
    await expect(page.getByTestId('search-panel')).toHaveCount(0);
    await expect(page.getByTestId('search-results')).toHaveCount(0);

    await pressMod(page, 'k');

    await expect(page.getByTestId('spotlight-search')).toHaveCount(0);
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByTestId('search-panel')).toHaveCount(0);
  });
});
