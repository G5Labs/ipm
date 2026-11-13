import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import type {
  RebaseChangeGraphNodeDetailsResponse,
  RebaseChangeGraphResponse,
  SourceControlChangeGraphResponse,
} from "../lib/types";
import {
  useRebaseChangeGraph,
  useRebaseChangeGraphNodeDetails,
  useSourceControlChangeGraph,
  useSourceControlChangeGraphNodeDetails,
} from "./useSourceControlChangeGraph";

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, retryDelay: 0, staleTime: Infinity },
    },
  });
}

function createWrapper(queryClient = createQueryClient()) {
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
}

function tooLargeResponse() {
  return HttpResponse.json(
    {
      error: {
        code: "DIFF_GRAPH_TOO_LARGE",
        message: "Workspace Diff Graph comparison is too large",
        details: {
          observedRoots: 128,
          maxRoots: 500,
          observedBytes: 130_000_000,
          maxBytes: 100 * 1024 * 1024,
        },
      },
    },
    { status: 413 },
  );
}

function response(comparison: "workspace" | "uncommitted") {
  return {
    revision: `${comparison}-revision`,
    comparison,
    range: {
      baseKind: comparison === "workspace" ? "merge_base" : "accepted_head",
      repositoryCount: 2,
    },
    nodes: [],
    edges: [],
    diagnostics: [],
    truncated: false,
  } satisfies SourceControlChangeGraphResponse;
}

describe("useSourceControlChangeGraph", () => {
  it("keys and requests topology independently by comparison and lens", async () => {
    const seen: string[] = [];
    const lensByComparison = new Map<string, string | null>();
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/source-control/change-graph",
        ({ request }) => {
          const params = new URL(request.url).searchParams;
          const comparison = params.get("comparison") as
            | "workspace"
            | "uncommitted";
          seen.push(comparison);
          lensByComparison.set(comparison, params.get("humanIntent"));
          return HttpResponse.json({ data: response(comparison) });
        },
      ),
    );

    const { result } = renderHook(
      () => [
        useSourceControlChangeGraph("ws-1", "workspace", false),
        useSourceControlChangeGraph("ws-1", "uncommitted", true),
      ],
      { wrapper: createWrapper() },
    );

    await waitFor(() => {
      expect(result.current[0].status).toBe("ok");
      expect(result.current[1].status).toBe("ok");
    });
    expect(seen.sort()).toEqual(["uncommitted", "workspace"]);
    // The boolean lens flag is serialized on every topology request.
    expect(lensByComparison.get("workspace")).toBe("false");
    expect(lensByComparison.get("uncommitted")).toBe("true");
  });

  it("requests both sides and revision-bound rebase graph details", async () => {
    const graph = {
      revision: "pair-rev-1",
      sourceWorkspace: { id: "source-1", name: "feature" },
      targetWorkspace: { id: "target-1", name: "main" },
      commonBaseRepositoryCount: 2,
      source: response("workspace"),
      target: response("workspace"),
      conflicts: [],
    } satisfies RebaseChangeGraphResponse;
    const nodeDetails = {
      revision: "pair-rev-1",
      artifactKey: "ontology-key",
      title: "Merge Workflow",
      artifact: "Ontology Concept",
      path: "core_workflow/merge_workflow",
      overlapLineCount: 0,
      overlapBlockIds: [],
      overlapBlockCount: 0,
    } satisfies RebaseChangeGraphNodeDetailsResponse;

    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/rebase-merge/change-graph",
        ({ request, params }) => {
          expect(params.workspaceId).toBe("source-1");
          expect(
            new URL(request.url).searchParams.get("targetWorkspaceId"),
          ).toBe("target-1");
          return HttpResponse.json({ data: graph });
        },
      ),
      http.get(
        "/api/v1/workspaces/:workspaceId/rebase-merge/change-graph/nodes/:artifactKey/details",
        ({ request, params }) => {
          const query = new URL(request.url).searchParams;
          expect(params.workspaceId).toBe("source-1");
          expect(params.artifactKey).toBe("ontology-key");
          expect(query.get("targetWorkspaceId")).toBe("target-1");
          expect(query.get("revision")).toBe("pair-rev-1");
          return HttpResponse.json({ data: nodeDetails });
        },
      ),
    );

    const { result } = renderHook(
      () => [
        useRebaseChangeGraph("source-1", "target-1"),
        useRebaseChangeGraphNodeDetails(
          "source-1",
          "target-1",
          "pair-rev-1",
          "ontology-key",
        ),
      ],
      { wrapper: createWrapper() },
    );

    await waitFor(() => {
      expect(result.current[0].status).toBe("ok");
      expect(result.current[1].status).toBe("ok");
    });
  });

  it("refreshes an open paired graph on a bounded interval", async () => {
    const graph = {
      revision: "pair-rev-1",
      sourceWorkspace: { id: "source-1", name: "feature" },
      targetWorkspace: { id: "target-1", name: "main" },
      commonBaseRepositoryCount: 2,
      source: response("workspace"),
      target: response("workspace"),
      conflicts: [],
    } satisfies RebaseChangeGraphResponse;
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/rebase-merge/change-graph",
        () => HttpResponse.json({ data: graph }),
      ),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(
        QueryClientProvider,
        { client: queryClient },
        children,
      );
    const { result } = renderHook(
      () => useRebaseChangeGraph("source-1", "target-1"),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    const query = queryClient.getQueryCache().find({
      queryKey: ["rebase-change-graph", "source-1", "target-1"],
      exact: true,
    });
    expect(
      (query?.options as { refetchInterval?: number } | undefined)
        ?.refetchInterval,
    ).toBe(30_000);
  });

  it("sends comparison and lens with revision-bound Node details", async () => {
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/source-control/change-graph/nodes/:nodeId/details",
        ({ request, params }) => {
          const query = new URL(request.url).searchParams;
          expect(query.get("comparison")).toBe("workspace");
          expect(query.get("revision")).toBe("rev-1");
          expect(query.get("humanIntent")).toBe("true");
          return HttpResponse.json({
            data: {
              revision: "rev-1",
              comparison: "workspace",
              nodeId: params.nodeId,
              kind: "unchanged_context",
            },
          });
        },
      ),
    );

    const { result } = renderHook(
      () =>
        useSourceControlChangeGraphNodeDetails(
          "ws-1",
          "workspace",
          "rev-1",
          "node-1",
          true,
        ),
      { wrapper: createWrapper() },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));
  });

  it("keeps a deterministic terminal error settled through revalidation", async () => {
    let requestCount = 0;
    let resolveRevalidation: ((response: Response) => void) | undefined;
    const revalidation = new Promise<Response>((resolve) => {
      resolveRevalidation = resolve;
    });
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/source-control/change-graph",
        () => {
          requestCount += 1;
          return requestCount === 1 ? tooLargeResponse() : revalidation;
        },
      ),
    );
    const queryClient = createQueryClient();
    const statuses: string[] = [];
    const { result } = renderHook(
      () => {
        const state = useSourceControlChangeGraph("ws-1", "workspace", false);
        statuses.push(state.status);
        return state;
      },
      { wrapper: createWrapper(queryClient) },
    );

    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(requestCount).toBe(1);
    const revalidationStart = statuses.length;
    void queryClient.invalidateQueries({
      queryKey: ["source-control-change-graph", "ws-1", "workspace"],
    });
    await waitFor(() => expect(requestCount).toBe(2));

    expect(result.current.status).toBe("error");
    expect(statuses.slice(revalidationStart)).not.toContain("loading");
    resolveRevalidation?.(tooLargeResponse());
    await waitFor(() => expect(requestCount).toBe(2));
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(statuses.slice(revalidationStart)).not.toContain("loading");
  });

  it("moves directly from a retained terminal error to graph data", async () => {
    let requestCount = 0;
    let resolveRevalidation: ((response: Response) => void) | undefined;
    const revalidation = new Promise<Response>((resolve) => {
      resolveRevalidation = resolve;
    });
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/source-control/change-graph",
        () => {
          requestCount += 1;
          return requestCount === 1 ? tooLargeResponse() : revalidation;
        },
      ),
    );
    const queryClient = createQueryClient();
    const statuses: string[] = [];
    const { result } = renderHook(
      () => {
        const state = useSourceControlChangeGraph("ws-1", "workspace", false);
        statuses.push(state.status);
        return state;
      },
      { wrapper: createWrapper(queryClient) },
    );

    await waitFor(() => expect(result.current.status).toBe("error"));
    const revalidationStart = statuses.length;
    void queryClient.invalidateQueries({
      queryKey: ["source-control-change-graph", "ws-1", "workspace"],
    });
    await waitFor(() => expect(requestCount).toBe(2));
    expect(result.current.status).toBe("error");

    resolveRevalidation?.(HttpResponse.json({ data: response("workspace") }));
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(statuses.slice(revalidationStart)).not.toContain("loading");
  });

  it("retries transient server failures once", async () => {
    let requestCount = 0;
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/source-control/change-graph",
        () => {
          requestCount += 1;
          return requestCount === 1
            ? HttpResponse.json(
                { error: { code: "INTERNAL", message: "try again" } },
                { status: 503 },
              )
            : HttpResponse.json({ data: response("workspace") });
        },
      ),
    );

    const { result } = renderHook(
      () => useSourceControlChangeGraph("ws-1", "workspace", false),
      { wrapper: createWrapper() },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(requestCount).toBe(2);
  });
});
