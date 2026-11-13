import { test, expect } from "@playwright/test";
import path from "path";
import { fileURLToPath } from "url";
import { loginWithOAuthStub, SEEDED_MEMBER_IDENTITY } from "../oauth";

const __esmDirname = path.dirname(fileURLToPath(import.meta.url));
const AUTH_FILE = path.join(__esmDirname, "..", ".auth", "user.json");

test.describe("Login Flow", () => {
  // Start with a fresh, unauthenticated browser context
  test.use({ storageState: { cookies: [], origins: [] } });

  test("unauthenticated user is redirected to landing page", async ({
    page,
  }) => {
    await page.goto("/");
    // Root auth flow currently lands on the first-party sign-in surface, but
    // older deployments may still pass through /landing to a public page.
    await page.waitForURL(
      (url) =>
        url.pathname.includes("/login") ||
        url.pathname.includes("/landing") ||
        url.pathname.includes("/coming-soon.html"),
      { timeout: 30_000 },
    );

    const currentPath = new URL(page.url()).pathname;
    const publicCta = currentPath.includes("/login")
      ? page
          .getByRole("link", { name: /continue with github/i })
          .or(page.getByRole("button", { name: /dev login/i }))
          .or(page.getByText(/no sign-in providers are configured/i).first())
      : page
          .getByRole("link", { name: /login|join waitlist/i })
          .or(page.getByText(/coming soon|waitlist|launch/i).first())
          .or(page.getByTestId("dev-login-button"))
          .or(page.getByRole("link", { name: /sign in with github/i }));

    await expect(publicCta.first()).toBeVisible({ timeout: 30_000 });
  });

  test("platform sign-in offers exactly the configured providers", async ({
    page,
  }) => {
    await page.goto("/login");
    const info = await page.request.get("/api/v1/instance/info");
    expect(info.ok()).toBeTruthy();
    const body = await info.json();
    const providers: string[] = (body.data ?? body).authProviders;
    // The e2e stack configures the GitHub stub; Microsoft Entra is shown only
    // when the instance reports it (G5_MICROSOFT_* with a tenant).
    expect(providers).toContain("github");
    for (const provider of providers) {
      expect(["github", "microsoft"]).toContain(provider);
    }
    await expect(
      page.getByRole("link", { name: /continue with github/i }),
    ).toBeVisible();
    await expect(page.locator('a[href^="/api/v1/auth/login/"]')).toHaveCount(
      providers.length,
    );
    await expect(
      page.getByRole("link", { name: /continue with microsoft/i }),
    ).toHaveCount(providers.includes("microsoft") ? 1 : 0);
    await expect(
      page.getByRole("link", { name: /continue with (google|gitlab)/i }),
    ).toHaveCount(0);
  });

  test("OAuth login flow completes and lands on projects page", async ({
    page,
  }) => {
    const useOAuth = process.env.E2E_USE_OAUTH === "true";
    const hasCredentials = !!process.env.VITE_E2E_USER;

    test.skip(
      !useOAuth && !hasCredentials,
      "Requires E2E_USE_OAUTH=true (local OAuth stub) or VITE_E2E_USER (real OAuth)",
    );

    if (useOAuth && !hasCredentials) {
      await loginWithOAuthStub(page, SEEDED_MEMBER_IDENTITY, {
        persistLocalStorageToken: false,
      });
      return;
    }

    // Navigate directly to /login — going to / would hit RequireAuth →
    // /landing → coming-soon.html (static page with no login button).
    await page.goto("/login");
    await page.waitForLoadState("domcontentloaded");

    // Click the GitHub sign-in link/button to start the OAuth flow
    const githubLink = page.getByRole("link", {
      name: /(?:continue|sign in) with github/i,
    });
    const githubButton = page.getByRole("button", {
      name: /continue with github/i,
    });
    const loginTrigger = githubLink.or(githubButton);
    await loginTrigger.click();

    // If using real OAuth with credentials, fill in the provider login form
    if (hasCredentials) {
      await page.getByLabel(/username|email/i).fill(process.env.VITE_E2E_USER!);
      await page.getByLabel(/password/i).fill(process.env.VITE_E2E_PASSWORD!);
      await page
        .getByRole("button", { name: /sign in|log in|authorize/i })
        .click();
    }
    // When E2E_USE_OAUTH=true, the OAuth stub auto-approves — no form interaction needed

    // Should land on an authenticated workspace page (/:org/:workspace)
    await page.waitForURL(
      (url) =>
        !url.pathname.includes("login") && /^\/[^/]+\/[^/]+/.test(url.pathname),
      { timeout: 30_000 },
    );
  });

  test("already authenticated user is redirected away from login", async ({
    browser,
  }) => {
    // Create a context with saved auth state
    const context = await browser.newContext({
      storageState: AUTH_FILE,
    });
    const page = await context.newPage();
    await page.goto("/login");
    // Should redirect to workspace, not stay on login
    await page.waitForURL((url) => !url.pathname.includes("login"));
    await context.close();
  });
});
