// =============================================================================
// Platform operator console — create an organization for an Admin
// =============================================================================
// Covers: the operator organization's Home at /operator/home
// (g5.organization.platform-operator.home) and its create form
// (g5.organization.platform-operator.create-org):
//   - the console lists tenant organizations
//   - an operator creates an organization for a named initial Admin; it is listed
//     and the operator gains no membership in it
//   - a non-operator is refused the platform API, and the retired self-serve
//     POST /api/v1/orgs no longer creates anything (g5.organization.creation)
//
// The operator persona `e2e-operator@g5labs.ai` exists only on stacks with
// G5_E2E_AUTH: the test-token path makes it the operator organization's Admin.
// =============================================================================

import { test, expect, type Page } from '@playwright/test';
import {
  APP_BASE_URL,
  assertActiveSubject,
  authedApiViaRequest,
  installIdentity,
} from '../helpers';

const OPERATOR_EMAIL = 'e2e-operator@g5labs.ai';

/** Sign this fresh context in as the fixture operator through test-token. */
async function signInAsOperator(page: Page): Promise<void> {
  const response = await page.request.post(
    new URL('/api/v1/auth/test-token', APP_BASE_URL).toString(),
    { data: { email: OPERATOR_EMAIL }, timeout: 10_000, failOnStatusCode: false },
  );
  expect(
    response.ok(),
    `test-token for ${OPERATOR_EMAIL} (needs G5_E2E_AUTH): ${await response.text()}`,
  ).toBeTruthy();
  const body = await response.json();
  const data = body.data ?? body;
  expect(data.jwt, 'test-token returned a jwt').toBeTruthy();
  expect(data.refreshToken, 'test-token returned a refreshToken').toBeTruthy();

  await page.goto('/login', { waitUntil: 'domcontentloaded' });
  await installIdentity(page, {
    jwt: data.jwt,
    refreshToken: data.refreshToken,
    email: OPERATOR_EMAIL,
  });
  await assertActiveSubject(page, 'operator console (test-token)');
}

async function openConsole(page: Page): Promise<void> {
  await page.goto('/operator/home', { waitUntil: 'domcontentloaded' });
  await expect(page.getByTestId('platform-home')).toBeVisible({ timeout: 30_000 });
  await expect(
    page.getByRole('heading', { name: 'Platform operator' }),
  ).toBeVisible();
}

function uniqueSuffix(): string {
  return `${Date.now().toString(36)}-${process.pid}-${Math.floor(Math.random() * 1e4)}`;
}

test.describe('Platform operator console', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    await signInAsOperator(page);
  });

  test('lists tenant organizations, not the operator organization', async ({ page }) => {
    await openConsole(page);

    const rows = page.getByTestId('platform-org-row');
    await expect(rows.first()).toBeVisible({ timeout: 15_000 });
    // The test-token fixtures always include g5labs.
    await expect(
      page.locator('[data-testid="platform-org-row"][data-org-slug="g5labs"]'),
    ).toBeVisible();
    await expect(
      page.locator('[data-testid="platform-org-row"][data-org-slug="operator"]'),
    ).toHaveCount(0);
    // Operators are not tenant members: organizations are not links.
    await expect(page.getByTestId('platform-orgs-table').getByRole('link')).toHaveCount(0);
    // The persona owns the operator organization, so it manages operators.
    await expect(page.getByTestId('platform-manage-operators')).toHaveAttribute(
      'href',
      '/operator/base?view=members',
    );
  });

  test('creates an organization for a new administrator account and lists it', async ({ page }) => {
    const suffix = uniqueSuffix();
    const name = `E2E Console ${suffix}`;
    const slug = `e2e-console-${suffix}`;
    const adminEmail = `e2e-console-admin-${suffix}@g5.dev`;

    await openConsole(page);
    await page.getByTestId('platform-create-org-name').fill(name);
    // The slug follows the name until it is edited.
    await expect(page.getByTestId('platform-create-org-slug')).toHaveValue(
      `e2e-console-${suffix}`.toLowerCase(),
    );
    await page.getByTestId('platform-create-org-slug').fill(slug);
    await page.getByTestId('platform-create-org-admin-email').fill(adminEmail);

    const created = page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === '/api/v1/platform/orgs',
    );
    await page.getByTestId('platform-create-org-submit').click();
    expect((await created).status()).toBe(201);

    const result = page.getByTestId('platform-create-org-result');
    await expect(result).toBeVisible({ timeout: 15_000 });
    await expect(result).toContainText(name);
    await expect(page.getByTestId('platform-create-org-admin')).toContainText(
      `${adminEmail}: new account`,
    );
    const row = page.locator(
      `[data-testid="platform-org-row"][data-org-slug="${slug}"]`,
    );
    await expect(row).toBeVisible({ timeout: 15_000 });
    await expect(row).toContainText(name);
    // Its one active member is the initial Admin.
    await expect(row.locator('td').last()).toHaveText('1');

    // The operator gains no membership in the organization it created.
    const me = await authedApiViaRequest(page, '/api/v1/users/me');
    expect(me.status).toBe(200);
    const slugs = (me.body?.data?.orgs ?? []).map(
      (org: { orgSlug: string }) => org.orgSlug,
    );
    expect(slugs).toContain('operator');
    expect(slugs).not.toContain(slug);
  });

  test('refuses a slug that already exists inline', async ({ page }) => {
    await openConsole(page);
    await page.getByTestId('platform-create-org-name').fill(`E2E Duplicate ${uniqueSuffix()}`);
    await page.getByTestId('platform-create-org-slug').fill('g5labs');
    await page
      .getByTestId('platform-create-org-admin-email')
      .fill(`e2e-duplicate-${uniqueSuffix()}@g5.dev`);
    await page.getByTestId('platform-create-org-submit').click();

    await expect(page.getByTestId('platform-create-org-error')).toContainText(
      'already exists',
      { timeout: 15_000 },
    );
    await expect(page.getByTestId('platform-create-org-result')).toHaveCount(0);
  });
});

// The shared storage state's identity is a tenant member, not an operator.
test.describe('Organization creation authority', () => {
  test('a non-operator is refused the platform API', async ({ page }) => {
    const list = await authedApiViaRequest(page, '/api/v1/platform/orgs');
    expect(list.status, JSON.stringify(list.body)).toBe(403);

    const create = await authedApiViaRequest(page, '/api/v1/platform/orgs', 'POST', {
      name: `E2E Refused ${uniqueSuffix()}`,
      adminEmail: `e2e-refused-${uniqueSuffix()}@g5.dev`,
    });
    expect(create.status, JSON.stringify(create.body)).toBe(403);
  });

  test('self-serve POST /api/v1/orgs no longer exists', async ({ page }) => {
    const slug = `e2e-self-serve-${uniqueSuffix()}`;
    const result = await authedApiViaRequest(page, '/api/v1/orgs', 'POST', {
      name: slug,
      slug,
    });
    // Listing stays on GET, so the removed POST is 405 (or 404).
    expect([404, 405], JSON.stringify(result.body)).toContain(result.status);
  });
});
