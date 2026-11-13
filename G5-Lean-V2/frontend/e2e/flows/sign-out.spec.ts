import { expect, test } from "@playwright/test";
import { loginWithOAuthStub, SEEDED_MEMBER_IDENTITY } from "../oauth";
import { APP_BASE_URL, REFRESH_COOKIE_NAME, jwtSubject } from "../helpers";

// Revoking the shared auth.setup session would invalidate unrelated specs.
test.use({ storageState: { cookies: [], origins: [] } });

test("legacy refresh cookies require sign-in and a fresh login recovers", async ({
  page,
  browser,
}) => {
  test.skip(process.env.E2E_USE_OAUTH !== "true", "Requires the OAuth stub");
  test.skip(
    !REFRESH_COOKIE_NAME.startsWith("__Host-"),
    "Requires secure cookies",
  );
  await loginWithOAuthStub(page, SEEDED_MEMBER_IDENTITY, {
    allowRootPath: true,
    persistLocalStorageToken: false,
  });
  const current = (await page.context().cookies()).find(
    (cookie) => cookie.name === REFRESH_COOKIE_NAME,
  );
  expect(current).toBeDefined();
  const initialRefresh = await page.request.post("/api/v1/auth/refresh", {
    data: {},
  });
  expect(initialRefresh.status()).toBe(200);
  const subject = jwtSubject((await initialRefresh.json()).data.jwt);
  const legacyName = REFRESH_COOKIE_NAME.replace(/^__Host-/, "");
  // Model a pre-cutover browser with no access token. A separate context keeps
  // the first login's in-flight auth bootstrap from repopulating localStorage.
  const context = await browser.newContext({
    baseURL: APP_BASE_URL,
    ignoreHTTPSErrors: true,
    storageState: { cookies: [], origins: [] },
  });
  try {
    await context.addCookies([
      {
        name: legacyName,
        value: current!.value,
        domain: new URL(APP_BASE_URL).hostname,
        path: "/api/v1/auth",
        httpOnly: true,
        secure: true,
        sameSite: "Lax",
      },
    ]);
    const recoveryPage = await context.newPage();
    const refused = await context.request.post("/api/v1/auth/refresh", {
      data: {},
    });
    expect(refused.status()).toBe(401);
    await recoveryPage.goto("/projects");
    await expect(recoveryPage).toHaveURL(/\/login(?:\?|$)/);
    await loginWithOAuthStub(recoveryPage, SEEDED_MEMBER_IDENTITY, {
      allowRootPath: true,
      persistLocalStorageToken: false,
    });
    const renewed = await recoveryPage.evaluate(async () => {
      const response = await fetch("/api/v1/auth/refresh", {
        method: "POST",
        credentials: "include",
      });
      return { status: response.status, body: await response.json() };
    });
    expect(renewed.status).toBe(200);
    expect(jwtSubject(renewed.body.data.jwt)).toBe(subject);
    const cookies = await context.cookies();
    expect(
      cookies.filter((cookie) => cookie.name === REFRESH_COOKIE_NAME),
    ).toHaveLength(1);
    expect(cookies.filter((cookie) => cookie.name === legacyName)).toHaveLength(0);
  } finally {
    await context.close();
  }
});

test("Sign Out revokes the browser session and stays signed out after reload", async ({
  page,
}) => {
  test.skip(process.env.E2E_USE_OAUTH !== "true", "Requires the OAuth stub");
  await loginWithOAuthStub(page, SEEDED_MEMBER_IDENTITY, {
    persistLocalStorageToken: false,
  });
  const refreshCookie = (await page.context().cookies()).find(
    (cookie) => cookie.name === REFRESH_COOKIE_NAME,
  );
  expect(refreshCookie).toBeDefined();
  const authenticatedPath = new URL(page.url()).pathname;
  await page.getByTestId("org-management-button").click();
  const logout = page.waitForResponse(
    (res) =>
      res.url().endsWith("/api/v1/auth/logout") &&
      res.request().method() === "POST",
  );
  await page.getByTestId("settings-sign-out").click();
  const response = await logout;
  expect(response.request().postData()).toBeNull();
  expect(response.status()).toBe(200);
  await expect(page).toHaveURL(/\/login$/);
  await expect(
    page.getByRole("link", { name: /continue with github/i }),
  ).toBeVisible();
  expect(
    (await page.context().cookies()).filter(
      (cookie) => cookie.name === REFRESH_COOKIE_NAME,
    ),
  ).toHaveLength(0);
  expect(
    await page.evaluate(() => localStorage.getItem("g5_auth_token")),
  ).toBeNull();
  const replay = await page.request.post("/api/v1/auth/refresh", {
    data: { refreshToken: refreshCookie!.value },
  });
  expect(replay.status()).toBe(401);

  await page.reload();
  await expect(page).toHaveURL(/\/login$/);
  await expect(
    page.getByRole("link", { name: /continue with github/i }),
  ).toBeVisible();
  await page.goto(authenticatedPath);
  await expect(page).toHaveURL(/\/login$/);
  await expect(
    page.getByRole("link", { name: /continue with github/i }),
  ).toBeVisible();
});
