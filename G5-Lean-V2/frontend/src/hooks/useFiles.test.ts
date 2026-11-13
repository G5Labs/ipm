// =============================================================================
// useFiles.test.ts — File Hook Integration Tests with MSW
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { describe, expect, it } from "vitest";
import type { FileTreeEntry } from "../lib/types";
import { server } from "../test/msw/server";
import {
  useFileTree,
  useFileTreeRead,
  useWorkspaceChanges,
  useWorkspaceChangesAction,
  useSaveWorkspaceFile,
} from "./useFiles";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });

  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);

  return { wrapper, queryClient };
}

describe("useFileTree", () => {
  it("does not request the file tree until enabled", async () => {
    const response: FileTreeEntry[] = [
      {
        name: "src",
        path: "src",
        type: "directory",
        size: null,
        children: [],
      },
    ];
    const requestedDepths: Array<string | null> = [];

    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/files",
        ({ request }) => {
          requestedDepths.push(new URL(request.url).searchParams.get("depth"));
          return HttpResponse.json({
            data: {
              entries: response,
              source: "flush",
              repoOid: "a".repeat(40),
              snapshotAt: "2026-09-21T00:00:00Z",
            },
          });
        },
      ),
    );

    const { wrapper } = createWrapper();
    const { result, rerender } = renderHook(
      ({ enabled }) =>
        useFileTree("workspace-1", "branch-main", undefined, -1, {
          enabled,
          refetchIntervalMs: false,
        }),
      { wrapper, initialProps: { enabled: false } },
    );

    await new Promise((resolve) => setTimeout(resolve, 25));
    expect(requestedDepths).toEqual([]);
    expect(result.current.status).toBe("loading");

    rerender({ enabled: true });

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(requestedDepths).toEqual(["-1"]);
    if (result.current.status === "ok") {
      expect(result.current.data).toEqual(response);
    }
  });
});

describe("useFileTreeRead", () => {
  it("keeps the source cut and the live fallback beside the entries", async () => {
    const entries: FileTreeEntry[] = [
      {
        name: "README.md",
        path: "README.md",
        type: "file",
        size: 12,
      },
    ];
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/repos/:repoId/files", () =>
        HttpResponse.json({
          data: {
            entries,
            source: "committed",
            repoOid: "a".repeat(40),
            snapshotAt: "2026-10-05T00:00:00Z",
            liveFallback: "entry_cap",
          },
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () =>
        useFileTreeRead("workspace-1", "branch-main", undefined, -1, {
          refetchIntervalMs: false,
        }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data).toEqual({
        entries,
        truncated: false,
        source: "committed",
        repoOid: "a".repeat(40),
        snapshotAt: "2026-10-05T00:00:00Z",
        liveFallback: "entry_cap",
      });
    }
  });

  it("keeps a live listing whose checkout HEAD was unreadable unlabeled", async () => {
    // The VM lists the checkout but read no HEAD: no OID rather than the
    // durable one (`g5.documents.read.labels`).
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/repos/:repoId/files", () =>
        HttpResponse.json({
          data: {
            entries: [],
            source: "live",
            repoOid: null,
            snapshotAt: "2026-10-05T00:00:00Z",
            liveFallback: null,
          },
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () =>
        useFileTreeRead("workspace-1", "branch-main", undefined, -1, {
          refetchIntervalMs: false,
        }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data.source).toBe("live");
      expect(result.current.data.repoOid).toBeNull();
      expect(result.current.data.liveFallback).toBeNull();
      expect(result.current.data.truncated).toBe(false);
    }
  });

  it("carries a committed listing's truncation to the tree", async () => {
    // The committed walk stopped at the tree's cap: the panel says the
    // folder has more rather than presenting the part as the whole.
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/repos/:repoId/files", () =>
        HttpResponse.json({
          data: {
            entries: [],
            truncated: true,
            source: "committed",
            repoOid: "a".repeat(40),
            snapshotAt: "2026-10-05T00:00:00Z",
            liveFallback: null,
          },
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () =>
        useFileTreeRead("workspace-1", "branch-main", undefined, -1, {
          refetchIntervalMs: false,
        }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data.truncated).toBe(true);
      expect(result.current.data.source).toBe("committed");
    }
  });
});

describe("useWorkspaceChanges", () => {
  function changesReport(attached: boolean) {
    return {
      workspace_id: "ws-1",
      source: attached ? "live" : "durable",
      attached,
      members: [],
    };
  }

  it("polls the live report at the requested interval while attached", async () => {
    let requests = 0;
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/changes", () => {
        requests += 1;
        return HttpResponse.json({ data: changesReport(true) });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useWorkspaceChanges("ws-1", { refetchIntervalMs: 20 }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    await waitFor(() => expect(requests).toBeGreaterThanOrEqual(3));
  });

  it("does not poll a durable report", async () => {
    let requests = 0;
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/changes", () => {
        requests += 1;
        return HttpResponse.json({ data: changesReport(false) });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useWorkspaceChanges("ws-1", { refetchIntervalMs: 20 }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    await new Promise((resolve) => setTimeout(resolve, 120));
    expect(requests).toBe(1);
  });
});

describe("useWorkspaceChangesAction", () => {
  it("refreshes Source control history and the diff graph after a commit", async () => {
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/changes/commit", () =>
        HttpResponse.json({
          data: {
            workspace_id: "ws-1",
            source: "live",
            attached: true,
            status: "succeeded",
            members: [],
          },
        }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["source-control-history", "r1", "b1"], {
      pages: [],
    });
    queryClient.setQueryData(["source-control-change-graph", "ws-1"], {});
    queryClient.setQueryData(["workspace-review-publications", "ws-1"], {});

    const { result } = renderHook(() => useWorkspaceChangesAction("ws-1"), {
      wrapper,
    });
    await result.current.mutateAsync({ action: "commit", message: "m" });

    for (const key of [
      ["source-control-history", "r1", "b1"],
      ["source-control-change-graph", "ws-1"],
      ["workspace-review-publications", "ws-1"],
    ]) {
      expect(queryClient.getQueryState(key)?.isInvalidated).toBe(true);
    }
  });
});

describe("shared file saves refresh question counts", () => {
  it.each([true, false])(
    "invalidates only affected aggregates after acknowledgement (accepted=%s)",
    async (accepted) => {
      let release!: () => void;
      const pending = new Promise<void>((resolve) => {
        release = resolve;
      });
      server.use(
        http.put(
          "/api/v1/workspaces/workspace-1/files/docs/workspace-1/questions.md",
          async () => {
            await pending;
            return accepted
              ? HttpResponse.json({
                  data: { hash: "after", size: 20, encoding: "utf-8" },
                })
              : HttpResponse.json(
                  {
                    error: {
                      code: "FILE_VERSION_CONFLICT",
                      message: "Changed",
                    },
                  },
                  { status: 409 },
                );
          },
        ),
      );
      const { wrapper, queryClient } = createWrapper();
      const affected = [
        "workspace-open-question-metrics",
        "org",
        ["workspace-1", "workspace-2"],
      ];
      const unrelated = [
        "workspace-open-question-metrics",
        "org",
        ["workspace-3"],
      ];
      queryClient.setQueryData(affected, []);
      queryClient.setQueryData(unrelated, []);
      const { result } = renderHook(() => useSaveWorkspaceFile(), { wrapper });
      const saving = result.current.mutateAsync({
        workspaceId: "workspace-1",
        repoId: "repo",
        path: "docs/workspace-1/questions.md",
        content: "Answered question",
        expectedHash: "before",
      });
      // An in-flight write must not announce a decision that is not yet saved.
      expect(queryClient.getQueryState(affected)?.isInvalidated).toBe(false);
      release();
      if (accepted) await saving;
      else await expect(saving).rejects.toThrow("Changed");
      expect(queryClient.getQueryState(affected)?.isInvalidated).toBe(accepted);
      expect(queryClient.getQueryState(unrelated)?.isInvalidated).toBe(false);
      queryClient.clear();
    },
  );
});
