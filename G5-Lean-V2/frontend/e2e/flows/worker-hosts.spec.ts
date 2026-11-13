import { expect, test, type Page } from '@playwright/test';
import {
  authedApi,
  openWorkspacePath,
  readCurrentOrgContext,
  TEST_ORG,
} from '../helpers';

// =============================================================================
// Worker-host provisioning E2E — org-scoped add/remove admin page.
//
// Implements views/test_views/worker_host_provisioning.test_view.md (§3). These
// scenarios require the demo stack (real API, Postgres, browser) and an
// org-admin session holding `workers:admin` with the cloud workflow flag on.
//
// host_jwt must NEVER appear in the DOM or storage (test-view I2).
// =============================================================================

// The Worker Hosts admin surface now renders as a center-panel tab inside the
// workspace chrome. Navigating to the standalone `/:org/settings/workers` route
// redirects into the org's base workspace (`?view=workers`) and opens the
// Workers tab; `openWorkspacePath` waits on the workspace shell, then we wait on
// the `Hosts` heading rendered by the tab content.
const WORKERS_PATH = `/${TEST_ORG}/settings/workers`;

async function openWorkerHostsPage(page: Page) {
  await openWorkspacePath(page, WORKERS_PATH);
  await expect(
    page.getByRole('heading', { name: 'Hosts', exact: true }),
  ).toBeVisible({ timeout: 30_000 });
}

test.describe('Worker hosts — org-scoped add/remove', () => {
  // These scenarios require the worker-host demo stack (cloud worker provisioning
  // wired + a workers:admin org-admin session) which the standard E2E lane does NOT
  // provision, so they are `not_yet_passing` (test_view §3). Skipped by default;
  // run explicitly with E2E_WORKER_HOSTS_LIVE=1 against a worker-host-capable stack.
  test.skip(
    !process.env.E2E_WORKER_HOSTS_LIVE,
    'Requires the worker-host demo stack; set E2E_WORKER_HOSTS_LIVE=1 (test_view §3, not_yet_passing).',
  );

  // §3.1 — admin provisions a host (region + default instance type).
  test('org admin provisions a worker host and sees a provisioning row', async ({
    page,
  }) => {
    await openWorkerHostsPage(page);

    await page.getByRole('button', { name: /add worker/i }).first().click();
    const dialog = page.getByRole('dialog', { name: /add worker host/i });
    await expect(dialog).toBeVisible({ timeout: 10_000 });

    // Default instance type is n2-standard-8; pick an explicit region.
    await dialog.getByLabel(/region/i).selectOption('us-central1');
    await expect(dialog.getByLabel(/instance type/i)).toHaveValue('n2-standard-8');
    await dialog.getByRole('button', { name: /provision host/i }).click();

    // A provisioning row appears and the dialog closes.
    await expect(dialog).not.toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/provisioning/i).first()).toBeVisible({
      timeout: 15_000,
    });

    // I2: the one-time host_jwt must never be surfaced.
    await expect(page.locator('body')).not.toContainText('hostJwt');
    const storage = await page.evaluate(() => JSON.stringify(window.localStorage));
    expect(storage).not.toContain('hostJwt');
    expect(storage).not.toContain('host_jwt');
  });

  // §3.3 — per-row remove → confirm → terminated.
  test('org admin removes a worker host per-row and it becomes terminated', async ({
    page,
  }) => {
    // Ensure at least one host exists via the API, then reload the page.
    const { orgId } = await readCurrentOrgContext(page);
    const provision = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/worker-hosts`,
      'POST',
      { region: 'us-central1' },
    );
    expect(provision.status).toBe(201);

    await openWorkerHostsPage(page);

    const row = page.getByTestId('worker-host-row').first();
    await expect(row).toBeVisible({ timeout: 15_000 });
    await row.getByRole('button', { name: /remove worker host/i }).click();

    const confirm = page.getByRole('dialog', {
      name: /(terminate host|cancel provisioning)\?/i,
    });
    await expect(confirm).toBeVisible({ timeout: 10_000 });
    await confirm
      .getByRole('button', { name: /(terminate host|cancel provisioning)/i })
      .click();

    // Reveal terminated rows and confirm the chip.
    await page.getByLabel(/show terminated/i).click();
    await expect(page.getByText(/terminated/i).first()).toBeVisible({
      timeout: 15_000,
    });
  });

  // §3.5 (backend authority) — a request to an org the user does NOT administer is
  // rejected (403/404). The no-admin / cloud-flag-off UI path (no controls, access
  // notice, absent links) is covered by the component test + I4; a live no-scope
  // session fixture is deferred.
  test('the backend rejects worker-host requests to a non-administered org', async ({ page }) => {
    await openWorkerHostsPage(page);
    const { orgId } = await readCurrentOrgContext(page);

    // A request to a *different* org the user does not administer must be
    // rejected by the backend (403/404), never silently accepted.
    const foreign = await authedApi(
      page,
      `/api/v1/orgs/org-does-not-belong/worker-hosts`,
      'POST',
      { region: 'us-central1' },
    );
    expect([403, 404]).toContain(foreign.status);

    // Sanity: the admin's own org accepts a well-formed provision.
    const ok = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/worker-hosts`,
      'POST',
      { region: 'us-central1' },
    );
    expect(ok.status).toBe(201);
  });

  // §3.6 (backend guard) — the UI prevents an empty/off-allowlist region via the
  // constrained <select> (asserted by the provision-flow + component tests), so this
  // only verifies the backend's defense-in-depth 422 for non-UI callers.
  test('the backend rejects an empty region with 422 (the UI select prevents it)', async ({ page }) => {
    await openWorkerHostsPage(page);
    const { orgId } = await readCurrentOrgContext(page);

    const result = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/worker-hosts`,
      'POST',
      { region: '' },
    );
    expect(result.status).toBe(422);
  });

  // §3.7 — list contract + terminated visibility.
  test('the list reflects the worker-hosts response and hides terminated by default', async ({
    page,
  }) => {
    const { orgId } = await readCurrentOrgContext(page);
    const list = await authedApi(page, `/api/v1/orgs/${orgId}/worker-hosts`);
    expect(list.status).toBe(200);
    const data = list.body?.data ?? list.body ?? {};
    expect(Array.isArray(data.hosts)).toBe(true);
    expect(typeof data.total).toBe('number');

    await openWorkerHostsPage(page);
    // The description remains count-free as the host list refreshes.
    await expect(page.getByText('Hosts run the workspace VMs for this organization.', { exact: true })).toBeVisible();
    await expect(page.getByTestId('worker-host-active-count')).toHaveCount(0);
  });
});
