import { expect, test } from "@playwright/test";

const fixture = "/e2e/browser/auth-continuity.html";
const token = (org: string) =>
  `e30.${Buffer.from(JSON.stringify({ sub: "alex", org, scopes: ["workspaces:read"], exp: 9999999999 })).toString("base64url")}.sig`;
const profile = (org: string) => ({
  id: "alex",
  email: "alex@example.com",
  displayName: "Alex",
  avatarUrl: null,
  activeOrgId: org,
  oauthProvider: "github",
  orgs: [],
  scmConnections: [],
  createdAt: "2026-01-01",
});

for (const path of ["silent", "periodic"] as const) {
  test(`two tabs retain chat and file drafts through ${path} organization adoption`, async ({
    page,
    context,
  }) => {
    let unavailable = false;
    let requestsFromOldPage = 0;
    await context.route("**/api/v1/auth/refresh", async (route) => {
      const org =
        route.request().postDataJSON()?.orgId ??
        /g5_refresh=(org-[ab])/.exec(
          route.request().headers()["cookie"] ?? "",
        )?.[1] ??
        "org-a";
      await route.fulfill({
        json: { data: { jwt: token(org) } },
        headers: {
          "Set-Cookie": `g5_refresh=${org}; Path=/api/v1/auth; HttpOnly; SameSite=Lax`,
        },
      });
    });
    await context.route("**/api/v1/users/me", async (route) => {
      const org =
        route.request().headers()["authorization"] ===
        `Bearer ${token("org-b")}`
          ? "org-b"
          : "org-a";
      if (route.request().frame().page() === page && org === "org-b") {
        requestsFromOldPage++;
        if (unavailable) {
          await route.fulfill({ status: 503 });
          return;
        }
      }
      await route.fulfill({ json: { data: profile(org) } });
    });
    await context.route("**/api/v1/continuity-probe", (route) =>
      route.fulfill({ status: 401 }),
    );
    await context.route("**/api/v1/auth/logout", (route) =>
      route.fulfill({ status: 503 }),
    );
    await page.clock.install();
    await page.goto(fixture);
    const chat = page.locator("textarea").last();
    await chat.fill("Unsent agent message");
    await page
      .getByLabel("File draft", { exact: true })
      .fill("Unsent file edit");
    await page.getByLabel("Organization comment").fill("Only organization A");
    await chat.evaluate((node) =>
      node.setAttribute("data-original-composer", "true"),
    );
    const other = await context.newPage();
    await other.goto(fixture);
    await other
      .getByRole("button", { name: "Organization B", exact: true })
      .click();
    await expect(other.getByLabel("Principal")).toHaveText("alex:org-b");
    unavailable = true;
    if (path === "silent")
      await page.getByRole("button", { name: "Refresh through API" }).click();
    else await page.clock.fastForward(50 * 60 * 1000);
    await expect(page.getByLabel("Principal")).toHaveText("alex:org-b");
    await expect.poll(() => requestsFromOldPage).toBeGreaterThan(0);
    await expect(page.getByText("Sign in", { exact: true })).toHaveCount(0);
    await expect(chat).toHaveValue("Unsent agent message");
    await expect(chat).toHaveAttribute("data-original-composer", "true");
    await expect(page.getByLabel("File draft", { exact: true })).toHaveValue(
      "Unsent file edit",
    );
    await expect(page.getByLabel("Organization comment")).toHaveValue("");
    unavailable = false;
    await page.clock.runFor(1100);
    await expect(page.getByLabel("Recovery")).toHaveText("ready");
    await expect(chat).toHaveValue("Unsent agent message");
    await page.getByRole("button", { name: "Sign out", exact: true }).click();
    await expect(page.getByText("Sign in", { exact: true })).toBeVisible();
    await expect(chat).toHaveCount(0);
    await other.close();
  });
}

test("initial refresh and profile outages recover without visiting sign-in", async ({
  page,
}) => {
  let refreshAvailable = false;
  let profileAvailable = false;
  await page.route("**/api/v1/auth/refresh", (route) =>
    route.fulfill(
      refreshAvailable
        ? { json: { data: { jwt: token("org-a") } } }
        : { status: 429 },
    ),
  );
  await page.route("**/api/v1/users/me", (route) =>
    profileAvailable
      ? route.fulfill({ json: { data: profile("org-a") } })
      : route.abort("failed"),
  );
  await page.clock.install();
  await page.goto(fixture);
  await expect(page.getByRole("status", { name: "Loading" })).toBeVisible();
  await expect(page.getByText("Sign in", { exact: true })).toHaveCount(0);
  refreshAvailable = true;
  await page.clock.runFor(1100);
  await expect(page.getByRole("status", { name: "Loading" })).toBeVisible();
  profileAvailable = true;
  await page.clock.runFor(1100);
  await expect(page.getByLabel("Principal")).toHaveText("alex:org-a");
  await expect(page.getByLabel("Recovery")).toHaveText("ready");
});

for (const temporaryFailure of [false, true]) {
  test(`a profile refusal after fresh renewal${temporaryFailure ? " and a temporary outage" : ""} reaches sign-in and stops retrying`, async ({
    page,
  }) => {
    let refreshes = 0;
    let profiles = 0;
    await page.route("**/api/v1/auth/refresh", (route) => {
      refreshes++;
      return route.fulfill({ json: { data: { jwt: token("org-a") } } });
    });
    await page.route("**/api/v1/users/me", (route) =>
      route.fulfill({ status: ++profiles === 2 && temporaryFailure ? 503 : 401 }),
    );
    await page.clock.install();
    await page.goto(fixture);
    if (temporaryFailure) {
      await expect.poll(() => profiles).toBe(2);
      await expect(page.getByText("Sign in", { exact: true })).toHaveCount(0);
      await page.clock.runFor(1100);
    }
    await expect(page.getByText("Sign in", { exact: true })).toBeVisible();
    await page.clock.runFor(60_000);
    expect(refreshes).toBe(2);
    await expect(page.getByLabel("Principal")).toHaveCount(0);
  });
}
