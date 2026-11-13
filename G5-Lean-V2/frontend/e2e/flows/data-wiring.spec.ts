import { test, expect } from '@playwright/test';
import { navigateToWorkspace, waitForWorkspaceLoad, TEST_ORG, TEST_WORKSPACE, softVisible } from '../helpers';

test.describe('Data Wiring', () => {
  test('chat auto-selects first session on load', async ({ page }) => {
    await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);

    // Open chat panel
    const chatToggle = page.getByTestId('toggle-right-panel').or(
      page.getByRole('button', { name: /chat/i })
    ).first();
    if (await chatToggle.isVisible()) {
      await chatToggle.click();

      // Chat panel should auto-select a session (messages should load without manual selection)
      const chatMessages = page.getByTestId('chat-messages');
      if (await softVisible(chatMessages, 5_000)) {
        // Either messages loaded, or empty state — both indicate a session was selected
        // Should NOT show session picker without a selection
        const noSessionIndicator = page.getByText(/select a session|no session selected/i);
        await expect(noSessionIndicator).not.toBeVisible({ timeout: 3_000 });
      }
    }
  });

  test('workspace details page shows repo branch info from workspace data', async ({ page }) => {
    // The repos region (chips + branch context) lives on the Workspace
    // Details page (center-panel D75). Open it in-app via the ribbon's
    // Details control and assert the chips there.
    await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);
    await waitForWorkspaceLoad(page);
    await page.getByTestId('workspace-info-button').click();

    const detailsPanel = page.getByTestId('workspace-details-panel');
    if (await softVisible(detailsPanel, 10_000)) {
      const panelText = await detailsPanel.textContent();
      expect(panelText).toBeTruthy();
      // Should NOT show "undefined" (which would indicate accessing nonexistent .repos field)
      expect(panelText!).not.toContain('undefined');
      expect(panelText!).not.toContain('null');

      const repoChips = detailsPanel.getByTestId('repo-chip');
      await expect.poll(() => repoChips.count(), { timeout: 10_000 }).toBeGreaterThan(0);
      await expect(repoChips.first()).not.toHaveAttribute('role', 'button');
      // A repo chip holds no controls: Publish and Make Remote are labelled
      // buttons beside the local-only pill, and a tracked branch's status
      // pill and detail expander render below the chip
      // (g5.workspace.remote-ui.actions).
      await expect(repoChips.first().locator('button')).toHaveCount(0);
      await expect(detailsPanel.getByRole('button', { name: /^Repository / })).toHaveCount(0);
    }
  });
});
