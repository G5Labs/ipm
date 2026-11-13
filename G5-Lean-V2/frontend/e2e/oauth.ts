import { expect, type Page, type Locator } from '@playwright/test';

import { EXPECTED_SUBJECT_KEY, recordExpectedSubject } from './helpers';

export interface OAuthStubIdentity {
  id: number;
  login: string;
  email: string;
  name: string;
  avatarUrl?: string;
}

const DEFAULT_STUB_BASE_URL = process.env.CI
  ? 'http://g5-test-oauth-stub:9876'
  // Prefer IPv4 locally because Playwright's API client can resolve localhost
  // to ::1 first, while the OAuth stub only listens on 127.0.0.1.
  : 'http://127.0.0.1:9876';
const E2E_IDENTITY_COOKIE = 'g5_e2e_oauth_identity';

// Hermetic fine-grained test PATs validated by the oauth-stub's /user endpoint
// (see docker/oauth-stub/server.js). G5_SCM_GITHUB_API_BASE_URL points the api
// at the stub in dev/CI so SCM connect is offline + deterministic.
export const E2E_VALID_PAT =
  process.env.OAUTH_STUB_PAT_VALID || 'github_pat_e2e_valid';
export const E2E_EXPIRED_PAT =
  process.env.OAUTH_STUB_PAT_EXPIRED || 'github_pat_e2e_expired';

/** Fill the GithubPatConnect widget within `scope` and submit it. */
export async function connectGithubPat(
  scope: Page | Locator,
  token: string,
): Promise<void> {
  await scope.getByLabel(/personal access token/i).fill(token);
  await scope.getByRole('button', { name: /^connect github$/i }).click();
}

export const SEEDED_MEMBER_IDENTITY: OAuthStubIdentity = {
  id: Number.parseInt(process.env.E2E_OAUTH_EXISTING_USER_ID ?? '100001', 10),
  login: process.env.E2E_OAUTH_EXISTING_USER_LOGIN ?? 'e2e-test-user',
  email: process.env.E2E_OAUTH_EXISTING_USER_EMAIL ?? 'e2e-test@g5.dev',
  name: process.env.E2E_OAUTH_EXISTING_USER_NAME ?? 'E2E Test User',
};

export const SEEDED_NEW_USER_IDENTITY: OAuthStubIdentity = {
  id: Number.parseInt(process.env.E2E_OAUTH_NEW_USER_ID ?? '100002', 10),
  login: process.env.E2E_OAUTH_NEW_USER_LOGIN ?? 'e2e-new-user',
  email: process.env.E2E_OAUTH_NEW_USER_EMAIL ?? 'e2e-new@g5.dev',
  name: process.env.E2E_OAUTH_NEW_USER_NAME ?? 'E2E New User',
};

/**
 * Deterministic non-owner member for owner-boundary specs (Z0). Invited into
 * the target org as `member` by the seeded owner and logged in through the
 * stub, so the identity, its role and its refresh credential are the same on
 * every run; never provisioned through test-token, which grants owner.
 */
export const SEEDED_FORK_MEMBER_IDENTITY: OAuthStubIdentity = {
  id: Number.parseInt(process.env.E2E_OAUTH_FORK_MEMBER_ID ?? '100003', 10),
  login: process.env.E2E_OAUTH_FORK_MEMBER_LOGIN ?? 'e2e-fork-member',
  email: process.env.E2E_OAUTH_FORK_MEMBER_EMAIL ?? 'e2e-fork-member@g5.dev',
  name: process.env.E2E_OAUTH_FORK_MEMBER_NAME ?? 'E2E Fork Member',
};

/**
 * Deterministic non-owner organization admin (issue #278). Invited into the
 * target org as `admin` by the seeded owner and logged in through the stub,
 * so it administers org rules without owning Base.
 */
export const SEEDED_ORG_ADMIN_IDENTITY: OAuthStubIdentity = {
  id: Number.parseInt(process.env.E2E_OAUTH_ORG_ADMIN_ID ?? '100004', 10),
  login: process.env.E2E_OAUTH_ORG_ADMIN_LOGIN ?? 'e2e-org-admin',
  email: process.env.E2E_OAUTH_ORG_ADMIN_EMAIL ?? 'e2e-org-admin@g5.dev',
  name: process.env.E2E_OAUTH_ORG_ADMIN_NAME ?? 'E2E Org Admin',
};

export function oauthStubBaseUrl(): string {
  return process.env.E2E_OAUTH_STUB_URL || DEFAULT_STUB_BASE_URL;
}

function encodeOAuthIdentityCookie(identity: OAuthStubIdentity): string {
  return Buffer.from(
    JSON.stringify({
      id: identity.id,
      login: identity.login,
      email: identity.email,
      name: identity.name,
      avatar_url: identity.avatarUrl,
    }),
    'utf8',
  ).toString('base64url');
}

async function setOAuthStubIdentityCookie(
  page: Page,
  identity: OAuthStubIdentity,
): Promise<void> {
  await page.context().addCookies([
    {
      url: oauthStubBaseUrl(),
      name: E2E_IDENTITY_COOKIE,
      value: encodeOAuthIdentityCookie(identity),
      sameSite: 'Lax',
    },
  ]);
}

async function clearOAuthStubIdentityCookie(page: Page): Promise<void> {
  await page.context().addCookies([
    {
      url: oauthStubBaseUrl(),
      name: E2E_IDENTITY_COOKIE,
      value: '',
      expires: 0,
    },
  ]);
}

export async function setOAuthStubIdentity(
  page: Page,
  identity: OAuthStubIdentity,
): Promise<void> {
  await setOAuthStubIdentityCookie(page, identity);

  const response = await page.request.post(`${oauthStubBaseUrl()}/__e2e/identity`, {
    data: {
      id: identity.id,
      login: identity.login,
      email: identity.email,
      name: identity.name,
      avatar_url: identity.avatarUrl,
    },
    timeout: 10_000,
  });

  expect(response.ok(), `failed to switch OAuth stub identity: ${await response.text()}`).toBeTruthy();
}

export async function resetOAuthStubIdentity(page: Page): Promise<void> {
  await clearOAuthStubIdentityCookie(page);

  const response = await page.request.post(`${oauthStubBaseUrl()}/__e2e/identity/reset`, {
    timeout: 10_000,
  });

  expect(response.ok(), `failed to reset OAuth stub identity: ${await response.text()}`).toBeTruthy();
}

export async function persistAuthTokenInLocalStorage(
  page: Page,
  email: string,
  options?: {
    orgSlug?: string;
  },
): Promise<void> {
  await page.evaluate(
    async ({
      loginEmail,
      targetOrgSlug,
    }: {
      loginEmail: string;
      targetOrgSlug?: string;
    }) => {
      async function refreshJwt(orgId?: string): Promise<string> {
        const body = orgId ? JSON.stringify({ orgId }) : undefined;
        const headers: Record<string, string> = {};
        if (body) {
          headers['Content-Type'] = 'application/json';
        }

        const res = await fetch('/api/v1/auth/refresh', {
          method: 'POST',
          credentials: 'include',
          headers,
          body,
        });
        if (!res.ok) {
          throw new Error(`refresh failed with ${res.status}`);
        }
        const json = await res.json().catch(() => ({}));
        const jwt = json.data?.jwt ?? json.jwt ?? '';
        if (!jwt) {
          throw new Error('refresh response did not include jwt');
        }
        return jwt;
      }

      let jwt = await refreshJwt();

      if (targetOrgSlug) {
        const meRes = await fetch('/api/v1/users/me', {
          method: 'GET',
          credentials: 'include',
          headers: {
            Authorization: `Bearer ${jwt}`,
          },
        });
        if (!meRes.ok) {
          throw new Error(`current-user lookup failed with ${meRes.status}`);
        }

        const meJson = await meRes.json().catch(() => ({}));
        const me = meJson.data ?? meJson ?? {};
        const orgs = Array.isArray(me.orgs) ? me.orgs : [];
        const matchedOrg = orgs.find((org: Record<string, unknown>) => {
          const slug = org.orgSlug ?? org.slug;
          return typeof slug === 'string' && slug === targetOrgSlug;
        });

        if (!matchedOrg) {
          throw new Error(`org slug ${targetOrgSlug} not found in current-user orgs`);
        }

        const targetOrgId = matchedOrg.orgId ?? matchedOrg.id;
        if (typeof targetOrgId !== 'string' || targetOrgId.length === 0) {
          throw new Error(`org slug ${targetOrgSlug} did not provide an org id`);
        }

        jwt = await refreshJwt(targetOrgId);
      }

      localStorage.setItem('g5_auth_token', jwt);
      localStorage.setItem('g5_auth_email', loginEmail);
    },
    { loginEmail: email, targetOrgSlug: options?.orgSlug },
  );
}

export async function loginWithOAuthStub(
  page: Page,
  identity: OAuthStubIdentity,
  options?: {
    allowRootPath?: boolean;
    persistLocalStorageToken?: boolean;
    targetOrgSlug?: string;
  },
): Promise<void> {
  const allowRootPath = options?.allowRootPath ?? false;
  const persistLocalStorageToken = options?.persistLocalStorageToken ?? true;

  await setOAuthStubIdentity(page, identity);

  await page.goto('/login');
  await page.waitForLoadState('domcontentloaded');

  await page.getByRole('link', { name: /continue with github/i }).click();

  // The local OAuth stub can either auto-approve or render a manual consent
  // screen depending on stack configuration. Handle both flows.
  const authorizeButton = page.getByRole('button', { name: /^authorize$/i });
  if (await authorizeButton.isVisible().catch(() => false)) {
    await authorizeButton.click();
  }

  await page.waitForURL((url) => {
    if (url.pathname.includes('login') || url.pathname.includes('landing')) {
      return false;
    }
    return allowRootPath || /^\/[^/]+\/[^/]+/.test(url.pathname);
  }, { timeout: 30_000 });

  // Identity contract (Z0): the callback just issued this context's one
  // refresh cookie for `identity`, so the intended subject is whatever the
  // bearer now resolves to; re-pin it, and drop a subject a previous login in
  // this context may have recorded, so later owner-only assertions compare
  // against this login and not an earlier one.
  if (persistLocalStorageToken) {
    await persistAuthTokenInLocalStorage(page, identity.email, {
      orgSlug: options?.targetOrgSlug,
    });
    await recordExpectedSubject(page);
  } else {
    await page.evaluate((key) => localStorage.removeItem(key), EXPECTED_SUBJECT_KEY);
  }
}
