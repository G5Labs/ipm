import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { delay, http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import { server } from "../test/msw/server";
import {
  makeIssueLink,
  makeIssueStateChange,
  makeIssueSummary,
} from "../test/externalIssueFixtures";
import type { WorkspaceIssueLinkListDTO } from "../lib/types";
import {
  externalIssueKeys,
  useChangeIssueState,
  useWorkspaceIssueLinks,
} from "./useExternalIssues";

// g5.external-issues.transition: the cached link follows GitHub's answer,
// and neither an older read nor a workspace switch can undo or misroute it.

function setup(workspaceId = "ws-1") {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  queryClient.setQueryData<WorkspaceIssueLinkListDTO>(
    externalIssueKeys.links("ws-1"),
    { links: [makeIssueLink()] },
  );
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const rendered = renderHook(
    ({ ws }: { ws: string }) => ({
      change: useChangeIssueState(ws),
      links: useWorkspaceIssueLinks(ws),
    }),
    { wrapper, initialProps: { ws: workspaceId } },
  );
  const cached = (ws: string) =>
    queryClient.getQueryData<WorkspaceIssueLinkListDTO>(
      externalIssueKeys.links(ws),
    );
  return { ...rendered, queryClient, cached };
}

describe("useChangeIssueState", () => {
  it("installs GitHub's answer and an older links read cannot restore the previous state", async () => {
    const posted: string[] = [];
    server.use(
      http.get("/api/v1/workspaces/ws-1/issue-links", async () => {
        await delay(200);
        return HttpResponse.json({
          data: { links: [makeIssueLink({ issue: makeIssueSummary() })] },
        });
      }),
      http.post(
        "/api/v1/workspaces/:workspaceId/issue-links/:linkId/state",
        async ({ request }) => {
          posted.push(new URL(request.url).pathname);
          return HttpResponse.json({ data: makeIssueStateChange("closed") });
        },
      ),
    );
    const { result, queryClient, cached } = setup();
    // A read starts before the write and would finish after it.
    void queryClient.refetchQueries({
      queryKey: externalIssueKeys.links("ws-1"),
    });
    await act(async () => {
      await result.current.change.mutateAsync({
        linkId: "link-1",
        state: "closed",
      });
    });
    expect(cached("ws-1")?.links[0].issue?.state).toBe("closed");
    await new Promise((resolve) => setTimeout(resolve, 350));
    expect(cached("ws-1")?.links[0].issue?.state).toBe("closed");
    expect(posted).toEqual([
      "/api/v1/workspaces/ws-1/issue-links/link-1/state",
    ]);
  });

  it("keeps a write bound to the workspace it was sent for after the page moves on", async () => {
    const posted: string[] = [];
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/issue-links", () =>
        HttpResponse.json({ data: { links: [] } }),
      ),
      http.post(
        "/api/v1/workspaces/:workspaceId/issue-links/:linkId/state",
        async ({ request }) => {
          posted.push(new URL(request.url).pathname);
          await delay(150);
          return HttpResponse.json({ data: makeIssueStateChange("closed") });
        },
      ),
    );
    const { result, rerender, cached } = setup();
    let pending: Promise<unknown> | undefined;
    act(() => {
      pending = result.current.change.mutateAsync({
        linkId: "link-1",
        state: "closed",
      });
    });
    rerender({ ws: "ws-2" });
    await act(async () => {
      await pending;
    });
    expect(posted).toEqual([
      "/api/v1/workspaces/ws-1/issue-links/link-1/state",
    ]);
    await waitFor(() =>
      expect(cached("ws-1")?.links[0].issue?.state).toBe("closed"),
    );
    expect(cached("ws-2")?.links ?? []).toEqual([]);
  });

  it("re-reads the links when the answer is lost, since the change may have applied", async () => {
    let reads = 0;
    server.use(
      http.get("/api/v1/workspaces/ws-1/issue-links", () => {
        reads += 1;
        return HttpResponse.json({
          data: {
            links: [
              makeIssueLink({ issue: makeIssueSummary({ state: "closed" }) }),
            ],
          },
        });
      }),
      http.post(
        "/api/v1/workspaces/:workspaceId/issue-links/:linkId/state",
        () => HttpResponse.error(),
      ),
    );
    const { result, cached } = setup();
    // The seeded cache is fresh: nothing is read before the write.
    expect(reads).toBe(0);
    const before = reads;
    await act(async () => {
      await expect(
        result.current.change.mutateAsync({
          linkId: "link-1",
          state: "closed",
        }),
      ).rejects.toBeTruthy();
    });
    await waitFor(() => expect(reads).toBeGreaterThan(before));
    await waitFor(() =>
      expect(cached("ws-1")?.links[0].issue?.state).toBe("closed"),
    );
  });

  it("updates every cached list that links the same issue, in any workspace", async () => {
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/issue-links/:linkId/state",
        () => HttpResponse.json({ data: makeIssueStateChange("closed") }),
      ),
    );
    // The other workspace's own read is in flight and would finish after
    // the write.
    server.use(
      http.get("/api/v1/workspaces/ws-2/issue-links", async () => {
        await delay(200);
        return HttpResponse.json({
          data: {
            links: [
              makeIssueLink({
                id: "link-9",
                workspaceId: "ws-2",
                role: "related",
              }),
            ],
          },
        });
      }),
    );
    const { result, queryClient, cached } = setup();
    queryClient.setQueryData<WorkspaceIssueLinkListDTO>(
      externalIssueKeys.links("ws-2"),
      {
        links: [
          makeIssueLink({ id: "link-9", workspaceId: "ws-2", role: "related" }),
          makeIssueLink({
            id: "link-8",
            workspaceId: "ws-2",
            issue: makeIssueSummary({
              externalIssueId: "900013",
              locator: { repositoryId: "11", number: 13 },
            }),
          }),
        ],
      },
    );
    void queryClient.refetchQueries({
      queryKey: externalIssueKeys.links("ws-2"),
    });
    await act(async () => {
      await result.current.change.mutateAsync({
        linkId: "link-1",
        state: "closed",
      });
    });
    expect(cached("ws-1")?.links[0].issue?.state).toBe("closed");
    const other = cached("ws-2")?.links ?? [];
    expect(other.find((link) => link.id === "link-9")?.issue?.state).toBe(
      "closed",
    );
    expect(other.find((link) => link.id === "link-8")?.issue?.state).toBe(
      "open",
    );
    // The cancelled read does not land afterwards and restore `open`.
    await new Promise((resolve) => setTimeout(resolve, 350));
    expect(
      (cached("ws-2")?.links ?? []).find((link) => link.id === "link-9")?.issue
        ?.state,
    ).toBe("closed");
  });

  it("leaves another workspace's first load alone, so it is not stranded", async () => {
    server.use(
      http.get("/api/v1/workspaces/ws-3/issue-links", async () => {
        await delay(150);
        return HttpResponse.json({
          data: {
            links: [makeIssueLink({ id: "link-3", workspaceId: "ws-3" })],
          },
        });
      }),
      http.post(
        "/api/v1/workspaces/:workspaceId/issue-links/:linkId/state",
        () => HttpResponse.json({ data: makeIssueStateChange("closed") }),
      ),
    );
    const { result, queryClient, cached } = setup();
    // A workspace the person just moved to is loading its list for the
    // first time while the write completes.
    const firstLoad = queryClient.fetchQuery({
      queryKey: externalIssueKeys.links("ws-3"),
      queryFn: () =>
        api.get<WorkspaceIssueLinkListDTO>(
          routes.workspaces.issueLinks("ws-3"),
        ),
    });
    await act(async () => {
      await result.current.change.mutateAsync({
        linkId: "link-1",
        state: "closed",
      });
    });
    await firstLoad;
    expect(cached("ws-1")?.links[0].issue?.state).toBe("closed");
    expect(cached("ws-3")?.links.map((link) => link.id)).toEqual(["link-3"]);
  });
});
