import { afterEach, expect, it, vi } from "vitest";
import { createApiClient } from "./api";

afterEach(() => vi.restoreAllMocks());

it("signals organization navigation on membership denial without logging the account out", async () => {
  const changed = vi.fn();
  window.addEventListener("g5:organization-access-changed", changed);
  vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(
      JSON.stringify({
        error: {
          code: "FORBIDDEN",
          message: "Membership required",
          details: { missing_scopes: ["org_membership"] },
        },
      }),
      { status: 403, headers: { "Content-Type": "application/json" } },
    ),
  );
  const onAuthFailure = vi.fn();
  const client = createApiClient({
    getToken: () =>
      `header.${btoa(JSON.stringify({ sub: "user", org: "org" }))}.signature`,
    setToken: vi.fn(),
    onAuthFailure,
  });
  await expect(
    client.get("/api/v1/workspaces/disabled-org-workspace"),
  ).rejects.toMatchObject({ status: 403 });
  expect(changed).toHaveBeenCalledOnce();
  expect(onAuthFailure).not.toHaveBeenCalled();
  window.removeEventListener("g5:organization-access-changed", changed);
});

it.each([
  ["json", "switch", false],
  ["raw", "switch", false],
  ["json", "logout", false],
  ["raw", "logout", false],
  ["json", "renew", true],
  ["raw", "renew", true],
] as const)(
  "scopes a late %s membership denial after %s",
  async (mode, transition, signal) => {
    const jwt = (org: string, iat: number) =>
      `header.${btoa(JSON.stringify({ sub: "user", org, iat }))}.signature`;
    let token: string | null = jwt("disabled-org", 1);
    const changed = vi.fn();
    const onAuthFailure = vi.fn();
    window.addEventListener("g5:organization-access-changed", changed);
    vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
      token =
        transition === "logout"
          ? null
          : jwt(transition === "switch" ? "other-org" : "disabled-org", 2);
      return new Response(
        JSON.stringify({
          error: {
            code: "FORBIDDEN",
            details: { missing_scopes: ["org_membership"] },
          },
        }),
        { status: 403, headers: { "Content-Type": "application/json" } },
      );
    });
    const client = createApiClient({
      getToken: () => token,
      setToken: vi.fn(),
      onAuthFailure,
    });
    try {
      await expect(
        mode === "raw"
          ? client.getRawResponse("/api/v1/workspaces/old-org")
          : client.get("/api/v1/workspaces/old-org"),
      ).rejects.toMatchObject({ status: 403 });
      expect(changed).toHaveBeenCalledTimes(signal ? 1 : 0);
      expect(onAuthFailure).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener("g5:organization-access-changed", changed);
    }
  },
);
