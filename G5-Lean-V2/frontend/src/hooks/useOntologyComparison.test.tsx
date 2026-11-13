import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse, delay } from "msw";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { server } from "../test/msw/server";
import { useOntologyComparison } from "./useOntologyComparison";
function wrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}
describe("ontology baseline request identity", () => {
  it("does not fetch while off and never mixes a delayed former repository/file reply", async () => {
    let calls = 0;
    server.use(
      http.get(
        "*/api/v1/workspaces/:workspace/source-control/ontology-baseline",
        async ({ request }) => {
          calls++;
          const url = new URL(request.url);
          const repo = url.searchParams.get("repoId");
          if (repo === "old") await delay(80);
          return HttpResponse.json({
            data: { content: repo, baselineCommit: repo, revision: repo },
          });
        },
      ),
    );
    const { result, rerender } = renderHook(
      ({ repo, path, enabled }) =>
        useOntologyComparison("ws", repo, path, enabled),
      {
        initialProps: {
          repo: "old",
          path: ".g5/ontology/a.md",
          enabled: false,
        },
        wrapper: wrapper(),
      },
    );
    expect(calls).toBe(0);
    rerender({ repo: "old", path: ".g5/ontology/a.md", enabled: true });
    await waitFor(() => expect(calls).toBe(1));
    rerender({ repo: "new", path: ".g5/ontology/b.md", enabled: true });
    expect(result.current.data).toBeUndefined();
    await waitFor(() => expect(result.current.data?.content).toBe("new"));
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(result.current.data?.baselineCommit).toBe("new");
  });
  it("reselects the baseline when the captured source revision advances", async () => {
    let calls = 0;
    server.use(
      http.get("*/api/v1/workspaces/ws/source-control/ontology-baseline", () =>
        HttpResponse.json({ data: { content: `version ${++calls}` } }),
      ),
    );
    const { result, rerender } = renderHook(
      ({ revision }) =>
        useOntologyComparison(
          "ws",
          "repo",
          ".g5/ontology/a.md",
          true,
          revision,
        ),
      {
        initialProps: { revision: "old" },
        wrapper: wrapper(),
      },
    );
    await waitFor(() => expect(result.current.data?.content).toBe("version 1"));
    rerender({ revision: "new" });
    expect(result.current.data).toBeUndefined();
    await waitFor(() => expect(result.current.data?.content).toBe("version 2"));
  });
});
