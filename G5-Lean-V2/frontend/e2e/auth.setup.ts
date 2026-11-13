import { test as setup, type BrowserContext, type Page } from '@playwright/test';
import { promises as fs } from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import {
  loginWithOAuthStub,
  persistAuthTokenInLocalStorage,
  SEEDED_MEMBER_IDENTITY,
} from './oauth';
import {
  assertActiveSubject,
  authedApiViaRequest,
  AUTH_TOKEN_KEY,
  EXPECTED_SUBJECT_KEY,
  installIdentity,
  jwtSubject,
  recordExpectedSubject,
  REFRESH_COOKIE_NAME,
  REFRESH_COOKIE_PATH,
} from './helpers';

const TEST_SELECTED_ORG = process.env.VITE_E2E_SELECTED_ORG ?? 'g5labs';

const __esmDirname = path.dirname(fileURLToPath(import.meta.url));
const authFile = path.join(__esmDirname, '.auth', 'user.json');

/**
 * The flows were written against the always-open full left sidebar, which is
 * no longer the default (ui g5.ui.sidebar-default: collapse-to-rail), and with the
 * Guided section shown, which is now off by default (ui g5.ui.guided-toggle).
 * Pin both in the shared storage state so every flow keeps clicking the
 * stacked panel and the Guided flows find their section; a flow that tests
 * another value patches it (`patchStoredUserSettings`).
 */
async function seedFlowUserSettings(page: Page): Promise<void> {
  await page.evaluate(() => {
    const key = 'g5-user-settings';
    let settings: Record<string, unknown> = {};
    try {
      settings = JSON.parse(localStorage.getItem(key) ?? '{}') ?? {};
    } catch {
      settings = {};
    }
    localStorage.setItem(
      key,
      JSON.stringify({
        ...settings,
        sidebarDisplayMode: 'expanded',
        showGuidedJourney: true,
      }),
    );
  });
}

async function persistVerifiedStorageState(
  context: BrowserContext,
  page: Page,
): Promise<void> {
  await fs.mkdir(path.dirname(authFile), { recursive: true });
  await fs.rm(authFile, { force: true });
  await seedFlowUserSettings(page);
  await context.storageState({ path: authFile });

  const saved = JSON.parse(await fs.readFile(authFile, 'utf8')) as {
    cookies?: Array<{ name?: string; path?: string }>;
    origins?: Array<{
      localStorage?: Array<{ name?: string; value?: string }>;
    }>;
  };

  // Identity contract (Z0): the saved state carries exactly one effective
  // refresh credential, at the API's cookie path, plus the bearer and the
  // subject it is expected to keep resolving to.
  const refreshCookiesSaved = saved.cookies?.filter(cookie => cookie.name === REFRESH_COOKIE_NAME) ?? [];
  if (refreshCookiesSaved.length !== 1 || refreshCookiesSaved[0].path !== REFRESH_COOKIE_PATH) {
    throw new Error(
      `E2E auth bootstrap saved ${refreshCookiesSaved.length} ${REFRESH_COOKIE_NAME} cookie(s) at ${refreshCookiesSaved.map(cookie => cookie.path).join(',') || '-'}; expected exactly one at ${REFRESH_COOKIE_PATH}`,
    );
  }
  const entries = saved.origins?.flatMap(origin => origin.localStorage ?? []) ?? [];
  const savedToken = entries.find(entry => entry.name === AUTH_TOKEN_KEY)?.value ?? '';
  const savedSubject = entries.find(entry => entry.name === EXPECTED_SUBJECT_KEY)?.value ?? '';
  if (!savedToken || !savedSubject) {
    throw new Error('E2E auth bootstrap saved incomplete storage state');
  }
  if (jwtSubject(savedToken) !== savedSubject) {
    throw new Error(
      `E2E auth bootstrap saved a bearer for ${jwtSubject(savedToken)} but recorded subject ${savedSubject}`,
    );
  }
}

/**
 * Authenticate via E2E test-token, local OAuth stub, or real OAuth provider.
 *
 * **Strategy 1 — test-token (local/dev fallback):**
 * When `G5_E2E_AUTH=true` and no `VITE_E2E_USER`, issues a JWT via
 * `POST /api/v1/auth/test-token`. Fastest path, no OAuth involved.
 *
 * **Strategy 2 — local OAuth stub (CI/default for browser E2E):**
 * Exercises the full OAuth browser flow against the local OAuth stub
 * server. The flow is: navigate to app → click "Continue with GitHub" →
 * browser redirected to OAuth stub (auto-approve) → redirected back
 * to callback → cookie set → SPA boots with cookie-based refresh.
 *
 * **Strategy 3 — real OAuth provider (VITE_E2E_USER):**
 * Goes through the real OAuth flow with provided credentials.
 */
/** The API resolves the active bearer to the intended login, not another fixture user. */
async function assertIntendedEmail(page: Page, email: string): Promise<void> {
  const me = await authedApiViaRequest(page, '/api/v1/users/me');
  if (me.status !== 200) {
    throw new Error(`auth setup: /users/me returned ${me.status}: ${JSON.stringify(me.body)}`);
  }
  const resolved = String(me.body?.data?.email ?? me.body?.email ?? '');
  if (resolved.toLowerCase() !== email.toLowerCase()) {
    throw new Error(`auth setup: active subject is ${resolved || '<unknown>'}, expected ${email}`);
  }
}

setup('authenticate', async ({ page, context }) => {
  const apiBase = process.env.E2E_API_URL || 'http://localhost:7500';
  const useOAuth = process.env.E2E_USE_OAUTH === 'true';

  // --- Strategy 1: E2E test-token (no OAuth required) ---
  if (!process.env.VITE_E2E_USER && !useOAuth) {
    try {
      const resp = await page.request.post(`${apiBase}/api/v1/auth/test-token`, {
        data: { email: 'e2e-test@g5.dev' },
        timeout: 10_000,
      });

      if (resp.ok()) {
        const body = await resp.json();
        const data = body.data ?? body;
        const jwt = data.jwt;
        const refreshToken = data.refreshToken;

        if (jwt) {
          // The test-token endpoint returns the refresh token in the JSON body
          // rather than as an httpOnly cookie (unlike the OAuth flow). Install
          // the identity through the one shared path: exactly one refresh cookie
          // at the API's own path, the bearer and login email
          // the app reads, and the intended subject for later assertions.
          if (!refreshToken) {
            throw new Error('E2E auth bootstrap: test-token returned no refreshToken');
          }
          await page.goto('/login', { waitUntil: 'domcontentloaded' });
          await installIdentity(page, {
            jwt,
            refreshToken,
            email: 'e2e-test@g5.dev',
          });
          await assertActiveSubject(page, 'auth setup (test-token)');

          // Reload into app shell using the stored JWT.
          await page.goto('/');
          await page.waitForURL(
            url => !url.pathname.includes('login') && !url.pathname.includes('landing'),
            { timeout: 15_000 },
          );

          // Save authenticated browser context (includes localStorage)
          await persistVerifiedStorageState(context, page);
          return;
        }
      }

      console.warn(`test-token endpoint returned ${resp.status()}: ${await resp.text()}`);
    } catch (err) {
      console.warn('test-token request failed:', err);
    }

    throw new Error(
      'E2E auth bootstrap failed: /api/v1/auth/test-token unavailable and OAuth not enabled (set E2E_USE_OAUTH=true or VITE_E2E_USER).',
    );
  }

  // --- Strategy 2: Local OAuth stub (full browser OAuth flow) ---
  if (useOAuth && !process.env.VITE_E2E_USER) {
    await loginWithOAuthStub(page, SEEDED_MEMBER_IDENTITY, {
      targetOrgSlug: TEST_SELECTED_ORG,
    });
    // The OAuth callback issued the one refresh cookie; pin the subject it
    // logged in as, and require that subject to be the intended member.
    await recordExpectedSubject(page);
    await assertActiveSubject(page, 'auth setup (oauth stub)');
    await assertIntendedEmail(page, SEEDED_MEMBER_IDENTITY.email);

    // Save authenticated browser context (includes httpOnly refresh cookie
    // AND the localStorage JWT for test helpers that use authedApi/apiCall).
    await persistVerifiedStorageState(context, page);
    return;
  }

  // --- Strategy 3: Real OAuth flow ---
  // Navigate directly to /login (see Strategy 2 comment for why not /).
  await page.goto('/login');
  await page.waitForLoadState('domcontentloaded');
  await page.getByRole('link', { name: /continue with github/i }).click();

  if (process.env.VITE_E2E_USER && process.env.VITE_E2E_PASSWORD) {
    await page.getByLabel(/username|email/i).fill(process.env.VITE_E2E_USER);
    await page.getByLabel(/password/i).fill(process.env.VITE_E2E_PASSWORD);
    await page.getByRole('button', { name: /sign in|log in|authorize/i }).click();
  }

  await page.waitForURL(
    url =>
      !url.pathname.includes('login') &&
      /^\/[^/]+\/[^/]+/.test(url.pathname),
    { timeout: 30_000 },
  );

  // Same localStorage bridging as Strategy 2 — see comment above.
  await persistAuthTokenInLocalStorage(
    page,
    process.env.VITE_E2E_USER ?? SEEDED_MEMBER_IDENTITY.email,
    { orgSlug: TEST_SELECTED_ORG },
  );
  await recordExpectedSubject(page);
  await assertActiveSubject(page, 'auth setup (oauth provider)');

  await persistVerifiedStorageState(context, page);
});
