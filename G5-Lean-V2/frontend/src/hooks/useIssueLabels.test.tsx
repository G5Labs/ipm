import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import {
  makeIssueDetail,
  makeIssueLabelChange,
  makeIssueLink,
  makeIssueSummary,
  makeLabelPage,
} from "../test/externalIssueFixtures";
import type {
  ExternalIssueDetailDTO,
  ExternalIssueSearchResponseDTO,
  WorkspaceIssueLinkListDTO,
} from "../lib/types";
import {
  externalIssueKeys,
  useChangeIssueLabels,
  useCreateIssueLabel,
  useEditIssueLabel,
  useIssueLabelCatalog,
} from "./useExternalIssues";

// g5.issue.labels.surfaces: a label change refreshes every cached copy of
// the issue; a catalog edit refreshes the catalog and the labels shown; the
// picker reads the catalog page by page, up to its cap.

const ORG = "org-g5labs";
const LOCATOR = { repositoryId: "11", number: 12 };

function setup() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  queryClient.setQueryData<ExternalIssueDetailDTO>(
    externalIssueKeys.detail(ORG, "11", 12),
    makeIssueDetail(),
  );
  queryClient.setQueryData<ExternalIssueSearchResponseDTO>(
    externalIssueKeys.search(ORG, "", "open", "any"),
    {
      results: [makeIssueSummary()],
      domainErrors: [],
      domainCount: 1,
      truncated: false,
    },
  );
  queryClient.setQueryData<WorkspaceIssueLinkListDTO>(
    externalIssueKeys.links("ws-1"),
    { links: [makeIssueLink()] },
  );
  queryClient.setQueryData(
    externalIssueKeys.labels(ORG, "domain-1"),
    makeLabelPage(),
  );
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const invalidated = (queryKey: readonly unknown[]) =>
    queryClient.getQueryState(queryKey)?.isInvalidated ?? false;
  return { queryClient, wrapper, invalidated };
}

describe("useChangeIssueLabels", () => {
  it("sends only the deltas, shows the labels as re-read and refreshes every copy of the issue", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `/api/v1/orgs/${ORG}/issues/github/11/12/labels`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: makeIssueLabelChange({
              changes: [
                { label: "wip", change: "add", outcome: "applied" },
                { label: "bug", change: "remove", outcome: "applied" },
              ],
              issue: makeIssueSummary({
                labels: [{ name: "wip", color: null }],
              }),
            }),
          });
        },
      ),
    );
    const { queryClient, wrapper, invalidated } = setup();
    const { result } = renderHook(() => useChangeIssueLabels(ORG), {
      wrapper,
    });
    await act(async () => {
      await result.current.mutateAsync({
        locator: LOCATOR,
        addLabels: ["wip"],
        removeLabels: ["bug"],
      });
    });
    expect(bodies).toEqual([{ addLabels: ["wip"], removeLabels: ["bug"] }]);
    expect(
      queryClient.getQueryData<ExternalIssueDetailDTO>(
        externalIssueKeys.detail(ORG, "11", 12),
      )?.labels,
    ).toEqual([{ name: "wip", color: null }]);
    // The page re-reads, and so do search rows (Home's tile) and links.
    expect(invalidated(externalIssueKeys.detail(ORG, "11", 12))).toBe(true);
    expect(invalidated(externalIssueKeys.search(ORG, "", "open", "any"))).toBe(
      true,
    );
    expect(invalidated(externalIssueKeys.links("ws-1"))).toBe(true);
    // A membership change leaves the catalog alone.
    expect(invalidated(externalIssueKeys.labels(ORG, "domain-1"))).toBe(false);
  });

  it("refreshes every copy of the issue after a refusal too", async () => {
    server.use(
      http.post(`/api/v1/orgs/${ORG}/issues/g5/12/labels`, () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "lost" } },
          { status: 503 },
        ),
      ),
    );
    const { wrapper, invalidated } = setup();
    const { result } = renderHook(() => useChangeIssueLabels(ORG), {
      wrapper,
    });
    await act(async () => {
      await result.current
        .mutateAsync({
          locator: { repositoryId: "g5", number: 12 },
          addLabels: ["wip"],
          removeLabels: [],
        })
        .catch(() => undefined);
    });
    expect(invalidated(externalIssueKeys.search(ORG, "", "open", "any"))).toBe(
      true,
    );
    expect(invalidated(externalIssueKeys.links("ws-1"))).toBe(true);
  });
});

describe("catalog writes", () => {
  it("a create refreshes only the catalog of the domain it was sent for", async () => {
    server.use(
      http.post(`/api/v1/orgs/${ORG}/issue-domains/domain-1/labels`, () =>
        HttpResponse.json(
          {
            data: {
              outcome: "applied",
              label: { id: "9", name: "docs", color: "0075ca" },
              githubLogin: "alex-gh",
              message: null,
            },
          },
          { status: 201 },
        ),
      ),
    );
    const { wrapper, invalidated } = setup();
    const { result } = renderHook(() => useCreateIssueLabel(ORG), {
      wrapper,
    });
    await act(async () => {
      await result.current.mutateAsync({ domainId: "domain-1", name: "docs" });
    });
    expect(invalidated(externalIssueKeys.labels(ORG, "domain-1"))).toBe(true);
    expect(invalidated(externalIssueKeys.detail(ORG, "11", 12))).toBe(false);
  });

  it("an edit refreshes the catalog and every issue that shows labels", async () => {
    server.use(
      http.patch(`/api/v1/orgs/${ORG}/issue-domains/domain-1/labels/101`, () =>
        HttpResponse.json({
          data: {
            outcome: "applied",
            label: { id: "101", name: "defect", color: "d73a4a" },
            githubLogin: "alex-gh",
            message: null,
          },
        }),
      ),
    );
    const { wrapper, invalidated } = setup();
    const { result } = renderHook(() => useEditIssueLabel(ORG), { wrapper });
    await act(async () => {
      await result.current.mutateAsync({
        domainId: "domain-1",
        labelId: "101",
        name: "defect",
      });
    });
    expect(invalidated(externalIssueKeys.labels(ORG, "domain-1"))).toBe(true);
    expect(invalidated(externalIssueKeys.detail(ORG, "11", 12))).toBe(true);
    expect(invalidated(externalIssueKeys.search(ORG, "", "open", "any"))).toBe(
      true,
    );
    expect(invalidated(externalIssueKeys.links("ws-1"))).toBe(true);
  });
});

describe("useIssueLabelCatalog", () => {
  it("reads every page", async () => {
    const pages: string[] = [];
    server.use(
      http.get(
        `/api/v1/orgs/${ORG}/issue-domains/domain-1/labels`,
        ({ request }) => {
          const page = new URL(request.url).searchParams.get("page") ?? "";
          pages.push(page);
          return HttpResponse.json({
            data: makeLabelPage({
              labels: [{ id: page, name: `label-${page}`, color: null }],
              nextPage: page === "1" ? 2 : null,
            }),
          });
        },
      ),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useIssueLabelCatalog(ORG, "domain-1"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(pages).toEqual(["1", "2"]);
    expect(
      result.current.status === "ok" &&
        result.current.data.labels.map((label) => label.name),
    ).toEqual(["label-1", "label-2"]);
    expect(
      result.current.status === "ok" && result.current.data.truncated,
    ).toBe(false);
  });

  it("stops after ten pages and says the catalog is truncated", async () => {
    let reads = 0;
    server.use(
      http.get(
        `/api/v1/orgs/${ORG}/issue-domains/domain-1/labels`,
        ({ request }) => {
          reads += 1;
          const page = Number(new URL(request.url).searchParams.get("page"));
          return HttpResponse.json({
            data: makeLabelPage({
              labels: [{ id: String(page), name: `l${page}`, color: null }],
              nextPage: page + 1,
            }),
          });
        },
      ),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useIssueLabelCatalog(ORG, "domain-1"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(reads).toBe(10);
    expect(
      result.current.status === "ok" && result.current.data.truncated,
    ).toBe(true);
  });
});
