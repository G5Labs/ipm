import { expect, test, type Page } from "@playwright/test";

import {
  APP_BASE_URL,
  assertActiveSubject,
  assertSingleRefreshCookie,
  authedApiViaRequest,
  expectedSubject,
  installRefreshCookie,
  jwtOrg,
  jwtSubject,
  readCurrentOrgContext,
  refreshCookies,
  REFRESH_COOKIE_NAME,
  switchAuthOrg,
  USING_TEST_TOKEN_AUTH,
} from "../helpers";
import { SEEDED_MEMBER_IDENTITY } from "../oauth";

/**
 * Browser OAuth and E2E fixture identity contract (Z0).
 *
 * Every browser context acts as exactly one platform user: the saved storage
 * state carries one refresh cookie at the API's own cookie path, the
 * bearer it holds resolves to the intended login, and neither an org switch
 * nor a request-side refresh may mint a token for a different subject. These
 * checks run against the shared fixture so an identity drift fails here, by
 * name, instead of as a bare 403 inside an owner-only workspace flow.
 */

// auth.setup.ts logs the test-token path in as this address, the stub path
// in as the seeded member, and a real OAuth provider in as VITE_E2E_USER.
const INTENDED_EMAIL = USING_TEST_TOKEN_AUTH
  ? "e2e-test@g5.dev"
  : (process.env.VITE_E2E_USER ?? SEEDED_MEMBER_IDENTITY.email);

async function refreshViaRequest(page: Page, orgId?: string): Promise<string> {
  const response = await page.request.post(
    new URL("/api/v1/auth/refresh", APP_BASE_URL).toString(),
    {
      failOnStatusCode: false,
      headers: { "Content-Type": "application/json" },
      data: orgId ? { orgId } : {},
    },
  );
  const text = await response.text();
  expect(response.status(), text).toBe(200);
  const json = JSON.parse(text);
  const jwt = String(json.data?.jwt ?? json.jwt ?? "");
  expect(jwt, "refresh returned no jwt").toBeTruthy();
  return jwt;
}

test.describe("E2E identity contract", () => {
  test("the saved state holds exactly one refresh cookie, at the API path", async ({
    page,
  }) => {
    await page.goto("/projects", { waitUntil: "domcontentloaded" });
    await assertSingleRefreshCookie(page.context(), "saved storage state");
  });

  test("the bearer resolves to the intended login", async ({ page }) => {
    const subject = await assertActiveSubject(page, "identity contract");
    const me = await authedApiViaRequest(page, "/api/v1/users/me");
    expect(me.status, JSON.stringify(me.body)).toBe(200);
    const user = me.body?.data ?? me.body ?? {};
    expect(String(user.id ?? "")).toBe(subject);
    expect(String(user.email ?? "").toLowerCase()).toBe(
      INTENDED_EMAIL.toLowerCase(),
    );
  });

  test("switching org keeps the subject", async ({ page }) => {
    const { orgId } = await readCurrentOrgContext(page);
    const before = await expectedSubject(page);
    await switchAuthOrg(page, orgId);
    const after = await assertActiveSubject(page, "org switch");
    expect(after).toBe(before);
    const jwt = await refreshViaRequest(page, orgId);
    expect(jwtSubject(jwt)).toBe(before);
    expect(jwtOrg(jwt)).toBe(orgId);
  });

  test("a request-side refresh keeps the subject", async ({ page }) => {
    await page.goto("/projects", { waitUntil: "domcontentloaded" });
    const intended = await expectedSubject(page);
    const jwt = await refreshViaRequest(page);
    expect(jwtSubject(jwt)).toBe(intended);
    // The helper that refreshes on the request side applies the same rule.
    const me = await authedApiViaRequest(page, "/api/v1/users/me");
    expect(me.status, JSON.stringify(me.body)).toBe(200);
    expect(String(me.body?.data?.id ?? me.body?.id ?? "")).toBe(intended);
  });

  test("installing an identity leaves no second refresh credential in effect", async ({
    page,
  }) => {
    await page.goto("/projects", { waitUntil: "domcontentloaded" });
    const context = page.context();
    const intended = await expectedSubject(page);
    const [current] = await refreshCookies(context);
    expect(current, "no refresh cookie in the saved state").toBeTruthy();

    // Old fixtures may retain the unprefixed cookie at a different path.
    // Secure browsers forbid a second path for the protected cookie itself.
    await context.addCookies([
      {
        name: REFRESH_COOKIE_NAME.replace(/^__Host-/, ""),
        value: "stray-refresh-credential",
        url: APP_BASE_URL,
        httpOnly: true,
        secure: new URL(APP_BASE_URL).protocol === "https:",
        sameSite: "Lax",
      },
    ]);
    expect((await refreshCookies(context)).length).toBe(2);

    await installRefreshCookie(context, current.value);
    await assertSingleRefreshCookie(context, "after installRefreshCookie");
    const jwt = await refreshViaRequest(page);
    expect(jwtSubject(jwt)).toBe(intended);
  });
});
