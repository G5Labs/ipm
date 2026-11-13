import { expect, request } from "@playwright/test";
import { APP_BASE_URL } from "./helpers";

/** Create a private test organization without changing the person's session. */
export async function createOrganizationFor(
  adminEmail: string,
  name: string,
): Promise<{ id: string; slug: string }> {
  // Only operators create organizations. This fixture persona exists only
  // on stacks with G5_E2E_AUTH; its session stays in a separate context.
  const api = await request.newContext({
    baseURL: APP_BASE_URL,
    ignoreHTTPSErrors: APP_BASE_URL.startsWith("https://"),
  });
  try {
    const token = await api.post("/api/v1/auth/test-token", {
      data: { email: "e2e-operator@g5labs.ai" },
      timeout: 10_000,
      failOnStatusCode: false,
    });
    expect(
      token.ok(),
      `operator test-token (needs G5_E2E_AUTH): ${await token.text()}`,
    ).toBeTruthy();
    const tokenBody = await token.json();
    const jwt = (tokenBody.data ?? tokenBody).jwt;
    expect(jwt, "test-token returned a jwt").toBeTruthy();

    const created = await api.post("/api/v1/platform/orgs", {
      headers: { Authorization: `Bearer ${jwt}` },
      data: { name, adminEmail },
      timeout: 30_000,
      failOnStatusCode: false,
    });
    expect(created.status(), await created.text()).toBe(201);
    const body = await created.json();
    const org = (body.data ?? body).org;
    expect(org.id, "created organization id").toBeTruthy();
    expect(org.slug, "created organization slug").toBeTruthy();
    return { id: org.id, slug: org.slug };
  } finally {
    await api.dispose();
  }
}
