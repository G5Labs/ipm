import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { expect, it, vi } from "vitest";
import { server } from "../test/msw/server";
import { useExternalIssuesFeature, useIssueDomains } from "./useExternalIssues";

let activeOrgId = "org1";
vi.mock("./useAuth", () => ({
  useAuth: () => ({ user: { activeOrgId } }),
}));
vi.mock("./useSettings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./useSettings")>()),
  useInstanceInfo: () => ({
    status: "ok",
    data: { features: ["external_issues"] },
  }),
}));

it("holds config and dependent issue reads until the active tenant matches even with cached enabled config", async () => {
  const config = {
    scope: "org",
    settings: { external_issues_enabled: true },
    secretKeys: [],
    effectiveSettings: { external_issues_enabled: true },
    effectiveSecretKeys: [],
  };
  const requests: string[] = [];
  server.use(
    http.get("/api/v1/orgs/org2/config", () => {
      requests.push(`config:${activeOrgId}`);
      return HttpResponse.json({ data: config });
    }),
    http.get("/api/v1/orgs/org2/issue-domains", () => {
      requests.push(`domains:${activeOrgId}`);
      return HttpResponse.json({ data: { domains: [] } });
    }),
  );
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  queryClient.setQueryData(["scoped-config", "org", "org2"], config);
  const { result, rerender } = renderHook(
    () => {
      const feature = useExternalIssuesFeature("org2");
      const domains = useIssueDomains("org2", feature.status === "enabled");
      return { feature, domains };
    },
    {
      wrapper: ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={queryClient}>
          {children}
        </QueryClientProvider>
      ),
    },
  );
  expect(result.current.feature.status).toBe("loading");
  expect(requests).toEqual([]);

  activeOrgId = "org2";
  rerender();
  await waitFor(() => expect(result.current.feature.status).toBe("enabled"));
  await waitFor(() =>
    expect(requests.sort()).toEqual(["config:org2", "domains:org2"]),
  );
  await waitFor(() => expect(result.current.domains.status).toBe("ok"));
  activeOrgId = "org1";
  rerender();
  expect(result.current.feature.status).toBe("loading");
});
