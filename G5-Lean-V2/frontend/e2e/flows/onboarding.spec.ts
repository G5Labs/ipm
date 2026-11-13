import { expect, test } from '@playwright/test';
import {
  resetOAuthStubIdentity,
  setOAuthStubIdentity,
  type OAuthStubIdentity,
} from '../oauth';

let freshIdentityCounter = 0;

function freshOnboardingIdentity(): OAuthStubIdentity {
  const counter = freshIdentityCounter++;
  const suffix = `${Date.now()}-${process.pid}-${counter}`;
  return {
    id: 200_000_000 + (Date.now() % 100_000_000) + counter,
    login: `e2e-new-${suffix}`,
    email: `e2e-new-${suffix}@g5.dev`,
    name: 'E2E New User',
  };
}

// Sign-in admits only an invited account or an organization member
// (g5.user.admission), and nobody creates their own organization
// (g5.organization.creation): a fresh identity is refused and named.
test.describe('OAuth onboarding', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.afterEach(async ({ page }) => {
    await resetOAuthStubIdentity(page);
  });

  test('an uninvited OAuth identity is refused on the denied page', async ({ page }) => {
    const identity = freshOnboardingIdentity();
    await setOAuthStubIdentity(page, identity);

    await page.goto('/login');
    await page.waitForLoadState('domcontentloaded');
    await page.getByRole('link', { name: /continue with github/i }).click();
    // The stub may render a manual consent screen depending on configuration.
    const authorizeButton = page.getByRole('button', { name: /^authorize$/i });
    if (await authorizeButton.isVisible().catch(() => false)) {
      await authorizeButton.click();
    }

    await page.waitForURL((url) => url.pathname === '/login/denied', {
      timeout: 30_000,
    });
    await expect(page.getByTestId('login-denied-email')).toContainText(
      identity.email,
    );
    await expect(page.getByTestId('login-denied-email')).toContainText(
      /no G5 invitation/i,
    );
    await expect(page.getByTestId('login-denied-different-account')).toBeVisible();
    await expect(page.getByRole('dialog', { name: /create organization/i })).toHaveCount(0);
  });
});

// Real browser routing against the production frontend, with only the public
// instance/session HTTP boundary stubbed. These tests need no OAuth credentials.
test.describe('Application entry routing', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test.beforeEach(async ({ page }) => {
    // Keep this routing-only fixture isolated from a running backend.
    await page.route('**/api/**', (route) => route.fulfill({ status: 404, json: {} }));
    await page.route('**/api/v1/instance/info', (route) => route.fulfill({
      json: { data: {
        version: 'test', features: [], authProviders: ['github'],
        maxUploadSizeMb: 100, selfHosted: false, devLoginEnabled: false, motd: null,
      } },
    }));
    await page.route('**/api/v1/auth/refresh', (route) => route.fulfill({ status: 401, json: {} }));
  });

  test('app root opens sign-in on the same origin', async ({ page, baseURL }) => {
    await page.goto('/');
    await expect(page).toHaveURL(new URL('/login', baseURL!).href);
    await expect(page.getByRole('link', { name: /continue with github/i }))
      .toHaveAttribute('href', '/api/v1/auth/login/github');
    expect(await page.evaluate(() => sessionStorage.getItem('g5_post_auth_redirect'))).toBeNull();
  });

  test('app root waits for session recovery before showing sign-in', async ({ page, baseURL }) => {
    let releaseRefresh!: () => void;
    let refreshRequested = false;
    const refreshPending = new Promise<void>((resolve) => { releaseRefresh = resolve; });
    await page.route('**/api/v1/auth/refresh', async (route) => {
      refreshRequested = true;
      await refreshPending;
      await route.fulfill({ status: 401, json: {} });
    });
    try {
      await page.goto('/');
      await expect.poll(() => refreshRequested).toBe(true);
      await expect(page.getByRole('status', { name: 'Loading page' })).toBeVisible();
      await expect(page).toHaveURL(new URL('/', baseURL!).href);
      await expect(page.getByRole('link', { name: /continue with github/i })).toHaveCount(0);
    } finally {
      releaseRefresh();
    }
    await expect(page).toHaveURL(new URL('/login', baseURL!).href);
  });

  // Only an account whose memberships are all inactive has no organization
  // after sign-in; it is asked for an invitation (g5.organization.creation).
  test('app root sends an authenticated account without an organization to /no-organization', async ({ page }) => {
    const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');
    const jwt = `${encode({ alg: 'HS256', typ: 'JWT' })}.${encode({
      sub: 'entry-user', org: '00000000-0000-0000-0000-000000000000',
      scopes: [], exp: Math.floor(Date.now() / 1000) + 3600,
    })}.routing-fixture`;
    await page.route('**/api/v1/auth/refresh', (route) => route.fulfill({ json: { data: { jwt } } }));
    await page.route('**/api/v1/users/me', (route) => route.fulfill({ json: { data: {
      id: 'entry-user', email: 'entry@g5labs.ai', displayName: 'Entry User',
      avatarUrl: null, oauthProvider: 'github', activeOrgId: null, orgs: [], scmConnections: [],
    } } }));
    await page.goto('/');
    await expect(page).toHaveURL(/\/no-organization$/);
    await expect(page.getByTestId('no-organization-email')).toContainText('entry@g5labs.ai');
    await expect(page.getByTestId('no-organization-different-account')).toBeVisible();
    await expect(page.getByRole('dialog', { name: /create organization/i })).toHaveCount(0);
  });

  for (const path of ['/landing', '/unknown/extra/fallback']) {
    test(`retains the separate public landing behavior for ${path}`, async ({ page }) => {
      await page.goto(path);
      await expect(page).toHaveURL(/\/coming-soon\.html$/);
      await expect(page).toHaveTitle('G5 Labs | Launching Soon');
    });
  }
});
