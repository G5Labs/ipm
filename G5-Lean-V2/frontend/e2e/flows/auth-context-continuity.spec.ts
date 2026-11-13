import { expect, test, type Page } from "@playwright/test";

import {
  authedApi,
  authedApiViaRequest,
  readCurrentOrgContext,
  switchAuthOrg,
} from "../helpers";

/**
 * Regression for issue #1853: sequential test contexts share one saved
 * Playwright storage state (localStorage JWT + httpOnly refresh cookie).
 * An earlier context may refresh the session and switch its org
 * (`switchAuthOrg` posts /api/v1/auth/refresh with an orgId, which updates
 * the stored refresh-token row), and a later fresh context can start with a
 * JWT that has aged out. The auth helpers must then recover through the
 * still-valid refresh cookie instead of surfacing an unretried 401 from
 * `/api/v1/users/me` during fixture setup.
 */
test.describe.configure({ mode: "serial" });

async function orgSwitchingContextKeepsSessionLive({
  page,
}: {
  page: Page;
}): Promise<void> {
  const context = await readCurrentOrgContext(page);
  expect(context.orgId).toBeTruthy();

  // Mirrors ensureAuthOrgForSlug: refresh with an explicit orgId, which
  // rewrites the shared refresh-token row's org binding server-side.
  await switchAuthOrg(page, context.orgId);

  const me = await authedApi(page, "/api/v1/users/me");
  expect(me.status, JSON.stringify(me.body).slice(0, 400)).toBe(200);
}

async function freshContextRecoversAuthAfterJwtAgedOut({
  page,
}: {
  page: Page;
}): Promise<void> {
  // Simulate the aged-context precondition: replace the storage-state JWT
  // with a dead value so the first authed call 401s and the helper must
  // recover via the g5_refresh cookie (in-browser refresh, then the
  // request-context fallback with bounded refresh retries).
  await page.goto("/projects", { waitUntil: "domcontentloaded" });
  await page.evaluate(() => {
    localStorage.setItem("g5_auth_token", "aged-out-e2e-token");
  });

  const me = await authedApi(page, "/api/v1/users/me");
  expect(me.status, JSON.stringify(me.body).slice(0, 400)).toBe(200);

  const viaRequest = await authedApiViaRequest(page, "/api/v1/users/me");
  expect(viaRequest.status, JSON.stringify(viaRequest.body).slice(0, 400)).toBe(
    200,
  );

  // The org-context fixture entry point that failed in #1853.
  const context = await readCurrentOrgContext(page);
  expect(context.orgId).toBeTruthy();
  expect(context.orgSlug).toBeTruthy();
}

test.describe("Auth context continuity", () => {
  test(
    "an org-switching context keeps its own session live",
    orgSwitchingContextKeepsSessionLive,
  );
  test(
    "a later fresh context recovers auth after its JWT aged out",
    freshContextRecoversAuthAfterJwtAgedOut,
  );
});
