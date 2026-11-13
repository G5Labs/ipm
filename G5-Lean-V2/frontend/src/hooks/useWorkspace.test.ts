// =============================================================================
// useWorkspace.test.ts — Workspace Hook Integration Tests with MSW
// =============================================================================

import { describe, it, expect, vi } from "vitest";
import React from "react";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import { ApiError } from "../lib/api";
import {
  useWorkspaces,
  workspaceOpenQuestionMetricsQueryOptions,
  useWorkspace,
  useWorkspaceStats,
  parseWorkspaceStats,
  usePendingSyncFiles,
  usePatchWorkspace,
  useCreateWorkspacePullRequest,
  useWorkspacePullRequestTargets,
  useDeleteWorkspace,
  useCloneWorkspace,
  resolveWorkspaceKey,
} from "./useWorkspace";
import { mockWorkspaces } from "../test/fixtures";
import type {
  WorkspaceSummary,
  WorkspaceDetail,
  WorkspaceStats,
  PendingSyncFile,
  WorkspacePullRequestSummary,
} from "../lib/types";

// ---------------------------------------------------------------------------
// Test Wrapper
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Mock Data
// ---------------------------------------------------------------------------

const mockWorkspaceDetail: WorkspaceDetail = {
  id: "ws-123",
  orgId: "org-1",
  name: "Test Workspace",
  isBase: false,
  description: "A test workspace",
  docsRepoId: null,

  createdBy: "user-alex",
  createdAt: "2024-11-15T09:00:00Z",
  updatedAt: "2024-11-15T10:00:00Z",
  pushMode: "manual",
  branches: [],
  clonedFromId: null,
  docsFolderPath: "docs/ws-123",
};

const mockStats: WorkspaceStats = {
  totalCostUsd: 12.5,
  tasksCompleted: 15,
  totalTasks: 20,
  concurrentTasks: 3,
  tasksStopped: 0,
  // This fixture has only retained task spend (8.0 + 4.5 = 12.5).
  byTask: [
    { taskId: "task-1", taskName: "sync main", taskType: "sync", costUsd: 8.0 },
    {
      taskId: "task-2",
      taskName: "chat session",
      taskType: "chat",
      costUsd: 4.5,
    },
  ],
  branches: [],
};

const mockDeps: string[] = ["ws-456", "ws-789"];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("useWorkspaces", () => {
  it("fetches workspace list for org", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", () =>
        HttpResponse.json({ data: mockWorkspaces }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspaces("org-1"), { wrapper });

    expect(result.current.status).toBe("loading");

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(Array.isArray(result.current.data)).toBe(true);
    }
  });

  it("returns loading then ok state transition", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", () =>
        HttpResponse.json({ data: mockWorkspaces }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspaces("org-1"), { wrapper });

    // Initially loading
    expect(result.current.status).toBe("loading");

    // Transitions to ok
    await waitFor(() => expect(result.current.status).toBe("ok"));
  });
});

describe("workspaceOpenQuestionMetricsQueryOptions", () => {
  it("is event-driven and does not poll every visible workspace", () => {
    const options = workspaceOpenQuestionMetricsQueryOptions("org-1", ["ws-1"]);

    expect(options.staleTime).toBe(30_000);
    expect(options).not.toHaveProperty("refetchInterval");
  });
});

describe("useWorkspace", () => {
  it("fetches single workspace detail", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id", ({ params }) =>
        HttpResponse.json({
          data: { ...mockWorkspaceDetail, id: params.id as string },
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspace("ws-123"), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.id).toBe("ws-123");
      expect(result.current.data.name).toBeDefined();
    }
  });

  it("handles 404 workspace not found", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id", () =>
        HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "Workspace not found",
              requestId: "req-1",
            },
          },
          { status: 404 },
        ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspace("nonexistent"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("error"));

    if (result.current.status === "error") {
      expect(result.current.error).toBeInstanceOf(ApiError);
      expect((result.current.error as ApiError).code).toBe("NOT_FOUND");
    }
  });
});

describe("usePendingSyncFiles", () => {
  it("fetches downlift pending sync files when enabled", async () => {
    const pending: PendingSyncFile[] = [
      {
        id: "doc:repo:docs/brief.md",
        targetKind: "doc",
        path: "docs/brief.md",
        repoId: "repo-1",
        repoName: "docs",
        branchName: "feat/docs",
        headCommit: "a".repeat(40),
        lastDownliftedCommitSha: null,
        targetDocRef: "overlay:repo:ZG9jcy9icmllZi5tZA",
      },
    ];
    server.use(
      http.get("/api/v1/workspaces/:id/sync/pending", ({ request }) => {
        expect(new URL(request.url).searchParams.get("direction")).toBe(
          "downlift",
        );
        return HttpResponse.json({ data: pending });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => usePendingSyncFiles("workspace-1", true),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data).toEqual(pending);
    }
  });

  it("does not fetch pending sync files while disabled", () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => usePendingSyncFiles("workspace-1", false),
      { wrapper },
    );

    expect(result.current.status).toBe("loading");
  });
});

describe("useWorkspaceStats", () => {
  it("fetches workspace stats", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id/stats", () =>
        HttpResponse.json({ data: mockStats }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspaceStats("ws-123"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.totalCostUsd).toBe(12.5);
      expect(result.current.data.totalTasks).toBe(20);
    }
  });

  it("reports a body that is not the stats DTO as an error (issue #733)", async () => {
    // A page loaded before a deploy still asks for `byTask` while the server
    // of the day named the list differently; the shape must surface as the
    // hook's error, not reach the spend table as something it cannot iterate.
    const { byTask, ...rest } = mockStats;
    server.use(
      http.get("/api/v1/workspaces/:id/stats", () =>
        HttpResponse.json({ data: { ...rest, byJob: byTask } }),
      ),
    );
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspaceStats("ws-123"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("error"));
    if (result.current.status === "error") {
      expect(result.current.error.message).toMatch(/byTask/);
    }
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("did not match"),
      expect.stringContaining("byTask"),
      expect.any(String),
      expect.stringContaining("byJob"),
    );
    warn.mockRestore();
  });

  it("parses the server DTO shape unchanged", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(parseWorkspaceStats(mockStats)).toEqual(mockStats);
    expect(() => parseWorkspaceStats(null)).toThrow(/did not match/);
    expect(() => parseWorkspaceStats({ ...mockStats, byTask: {} })).toThrow(
      /byTask/,
    );
    warn.mockRestore();
  });
});

describe("useDeleteWorkspace", () => {
  it("deletes the workspace and invalidates workspace caches", async () => {
    let deleteCalled = false;
    server.use(
      http.post(
        "/api/v1/workspaces/:id/delete",
        async ({ params, request }) => {
          deleteCalled = params.id === "ws-delete";
          expect(await request.json()).toEqual({
            force: false,
            operationId: expect.any(String),
          });
          return new HttpResponse(null, { status: 204 });
        },
      ),
    );

    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["workspace", "ws-delete"], mockWorkspaceDetail);
    queryClient.setQueryData(["workspaces"], mockWorkspaces);
    queryClient.setQueryData(["workspaces", "org-1"], mockWorkspaces);

    const { result } = renderHook(
      () => useDeleteWorkspace("ws-delete", "org-1"),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync(undefined);
    });

    expect(deleteCalled).toBe(true);
    await waitFor(() => {
      expect(
        queryClient.getQueryState(["workspace", "ws-delete"])?.isInvalidated,
      ).toBe(true);
      expect(queryClient.getQueryState(["workspaces"])?.isInvalidated).toBe(
        true,
      );
      expect(
        queryClient.getQueryState(["workspaces", "org-1"])?.isInvalidated,
      ).toBe(true);
    });
  });
});

describe("workspace name resolutions", () => {
  function seedResolutions(queryClient: QueryClient) {
    queryClient.setQueryData(resolveWorkspaceKey("acme", "issue-19"), {
      orgId: "org-1",
      workspaceId: "ws-old",
    });
    queryClient.setQueryData(resolveWorkspaceKey("acme", "Other"), {
      orgId: "org-1",
      workspaceId: "ws-other",
    });
  }

  it("deleting a workspace forgets the names that resolve to it", async () => {
    server.use(
      http.post(
        "/api/v1/workspaces/ws-old/delete",
        () => new HttpResponse(null, { status: 204 }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    seedResolutions(queryClient);

    const { result } = renderHook(() => useDeleteWorkspace("ws-old", "org-1"), {
      wrapper,
    });
    await act(async () => {
      await result.current.mutateAsync(undefined);
    });

    expect(
      queryClient.getQueryData(resolveWorkspaceKey("acme", "issue-19")),
    ).toBeUndefined();
    expect(
      queryClient.getQueryData(resolveWorkspaceKey("acme", "Other")),
    ).toEqual({ orgId: "org-1", workspaceId: "ws-other" });
  });

  it("forking forgets a stale resolution of the new name, ignoring case", async () => {
    server.use(
      http.post("/api/v1/workspaces/ws-base/clone", () =>
        HttpResponse.json(
          {
            data: {
              workspace: {
                ...mockWorkspaces[0],
                id: "ws-new",
                name: "Issue-19",
              },
              clonedBranches: 0,
              clonedDrafts: 0,
              carriedSessions: 0,
              issueLink: null,
            },
          },
          { status: 201 },
        ),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    seedResolutions(queryClient);

    const { result } = renderHook(() => useCloneWorkspace("ws-base"), {
      wrapper,
    });
    await act(async () => {
      await result.current.mutateAsync({
        name: "Issue-19",
        idempotencyKey: "fork-1",
      });
    });

    expect(
      queryClient.getQueryData(resolveWorkspaceKey("acme", "issue-19")),
    ).toBeUndefined();
    expect(
      queryClient.getQueryData(resolveWorkspaceKey("acme", "Other")),
    ).toEqual({ orgId: "org-1", workspaceId: "ws-other" });
  });
});

describe("usePatchWorkspace", () => {
  it("patches workspace and invalidates detail + list", async () => {
    const patchedWorkspace: WorkspaceSummary = {
      id: mockWorkspaceDetail.id,
      name: mockWorkspaceDetail.name,
      description: "Updated description",
      isBase: mockWorkspaceDetail.isBase,
      createdBy: mockWorkspaceDetail.createdBy,
      clonedFromId: mockWorkspaceDetail.clonedFromId,
      branches: mockWorkspaceDetail.branches,
      tasksCompleted: 0,
      totalTasks: 0,
      runningTime: null,
      cost: 0,
      concurrentTasks: 0,
      lastUpdated: mockWorkspaceDetail.updatedAt,
      createdAt: mockWorkspaceDetail.createdAt,
    };

    server.use(
      http.patch("/api/v1/workspaces/:id", () =>
        HttpResponse.json({ data: patchedWorkspace }),
      ),
    );

    const { wrapper, queryClient } = createWrapper();

    // Pre-populate caches
    queryClient.setQueryData(["workspace", "ws-123"], mockWorkspaceDetail);
    queryClient.setQueryData(["workspaces", "org-1"], mockWorkspaces);

    const { result } = renderHook(() => usePatchWorkspace("ws-123"), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({ description: "Updated description" });
    });

    // Verify workspace detail cache was invalidated
    await waitFor(() => {
      const state = queryClient.getQueryState(["workspace", "ws-123"]);
      expect(state?.isInvalidated).toBe(true);
    });
  });
});

describe("useCreateWorkspacePullRequest", () => {
  it("replaces cached PR snapshots and cancels older reads before exposing success", async () => {
    const old = {
      id: "pr",
      sourceWorkspaceId: "source",
      targetWorkspaceId: "target",
      title: "Old",
    } as WorkspacePullRequestSummary;
    const updated = { ...old, title: "Accepted" };
    const unrelated = { ...old, id: "other" };
    server.use(
      http.post("/api/v1/workspaces/source/pull-requests", () =>
        HttpResponse.json({
          data: { pullRequest: updated, alreadyExisted: true, dirtyDrafts: [] },
        }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const sourceKey = ["workspace-pull-requests", "source", "source", "open"];
    const targetKey = ["workspace-pull-requests", "target", "target", "open"];
    const closedKey = ["workspace-pull-requests", "source", "source", "closed"];
    for (const key of [sourceKey, targetKey])
      queryClient.setQueryData(key, [old, unrelated]);
    queryClient.setQueryData(closedKey, []);
    let aborted = false;
    let release!: (rows: WorkspacePullRequestSummary[]) => void;
    const olderRead = queryClient
      .fetchQuery({
        queryKey: sourceKey,
        staleTime: 0,
        queryFn: ({ signal }) => {
          signal.addEventListener("abort", () => {
            aborted = true;
          });
          return new Promise<WorkspacePullRequestSummary[]>((resolve) => {
            release = resolve;
          });
        },
      })
      .catch(() => undefined);
    const { result } = renderHook(
      () => useCreateWorkspacePullRequest("source"),
      { wrapper },
    );
    await act(async () => {
      await result.current.mutateAsync({ title: "Accepted" });
    });
    expect(aborted).toBe(true);
    release([old]);
    await olderRead;
    for (const key of [sourceKey, targetKey])
      expect(queryClient.getQueryData(key)).toEqual([updated, unrelated]);
    expect(queryClient.getQueryData(closedKey)).toEqual([]);
  });

  it("exposes partial provider failure and refreshes both PR lists after the local PR was saved", async () => {
    const message =
      "G5 pull request saved-pr was saved, but provider synchronization was not confirmed. Inspect PR state before retrying.";
    server.use(
      http.post("/api/v1/workspaces/:id/pull-requests", () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message,
              requestId: "partial-pr-test",
              details: {
                kind: "pull_request_provider_sync_incomplete",
                pullRequestId: "saved-pr",
                providerSyncOutcome: "unknown",
              },
            },
          },
          { status: 409 },
        ),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    for (const id of ["source", "target"]) {
      queryClient.setQueryData(["workspace-pull-requests", id], []);
    }
    const { result } = renderHook(
      () => useCreateWorkspacePullRequest("source"),
      { wrapper },
    );
    await act(async () => {
      await expect(
        result.current.mutateAsync({ targetWorkspaceId: "target" }),
      ).rejects.toThrow(message);
    });
    await waitFor(() => {
      expect(result.current.error?.message).toBe(message);
      for (const id of ["source", "target"]) {
        expect(
          queryClient.getQueryState(["workspace-pull-requests", id])
            ?.isInvalidated,
        ).toBe(true);
      }
    });
  });
});

it("uses the shared PR target order without filtering or reordering it", async () => {
  const targets = [
    { id: "parent", name: "Z parent" },
    { id: "base", name: "A base" },
  ];
  server.use(
    http.get("/api/v1/workspaces/:id/pull-requests/targets", () =>
      HttpResponse.json({ data: targets }),
    ),
  );
  const { wrapper } = createWrapper();
  const { result } = renderHook(
    () => useWorkspacePullRequestTargets("source"),
    { wrapper },
  );
  await waitFor(() => expect(result.current.status).toBe("ok"));
  if (result.current.status === "ok")
    expect(result.current.data).toEqual(targets);
});

it.each([503, 403, 404])(
  "retains loaded PR destinations only for transient refresh failures (%s)",
  async (status) => {
    const targets = [{ id: "base", name: "Base" }];
    let fail = false;
    server.use(
      http.get("/api/v1/workspaces/:id/pull-requests/targets", () =>
        fail
          ? HttpResponse.json(
              {
                error: {
                  code: "UNAVAILABLE",
                  message: "Destinations unavailable",
                },
              },
              { status },
            )
          : HttpResponse.json({ data: targets }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => useWorkspacePullRequestTargets("source"),
      { wrapper },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));
    fail = true;
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["workspaces", "pull-request-targets", "source"],
      });
    });
    if (status === 503) {
      await waitFor(() =>
        expect(result.current).toMatchObject({
          status: "ok",
          data: targets,
          refreshError: expect.any(ApiError),
        }),
      );
    } else {
      await waitFor(() =>
        expect(result.current).toMatchObject({
          status: "error",
          error: expect.any(ApiError),
        }),
      );
      expect(result.current).not.toHaveProperty("data");
    }
  },
);

it("sends title without a target and invalidates the server-resolved destination", async () => {
  server.use(
    http.post("/api/v1/workspaces/:id/pull-requests", async ({ request }) => {
      expect(await request.json()).toEqual({ title: "Explicit title" });
      return HttpResponse.json({
        data: {
          pullRequest: {
            id: "pr",
            targetWorkspaceId: "resolved",
            status: "open",
          },
        },
      });
    }),
  );
  const { wrapper, queryClient } = createWrapper();
  queryClient.setQueryData(["workspace", "resolved"], {});
  const { result } = renderHook(() => useCreateWorkspacePullRequest("source"), {
    wrapper,
  });
  await act(async () => {
    await result.current.mutateAsync({ title: "Explicit title" });
  });
  expect(
    queryClient.getQueryState(["workspace", "resolved"])?.isInvalidated,
  ).toBe(true);
});

it("refreshes PR target choices after a workspace mutation without refocusing", async () => {
  let targets = [
    { id: "parent", name: "Parent" },
    { id: "base", name: "Base" },
  ];
  let reads = 0;
  server.use(
    http.get("/api/v1/workspaces/:id/pull-requests/targets", () => {
      reads++;
      return HttpResponse.json({ data: targets });
    }),
    http.post("/api/v1/workspaces/parent/delete", () => {
      targets = [{ id: "base", name: "Base" }];
      return HttpResponse.json({ data: null });
    }),
  );
  const { wrapper } = createWrapper();
  const { result } = renderHook(
    () => ({
      targets: useWorkspacePullRequestTargets("source"),
      remove: useDeleteWorkspace("parent"),
    }),
    { wrapper },
  );
  await waitFor(() => expect(result.current.targets.status).toBe("ok"));
  await act(async () => {
    await result.current.remove.mutateAsync();
  });
  await waitFor(() => {
    expect(result.current.targets.status).toBe("ok");
    if (result.current.targets.status === "ok")
      expect(result.current.targets.data).toEqual([
        { id: "base", name: "Base" },
      ]);
  });
  expect(reads).toBe(2);
});

it("retries loaded transient destinations on each successful detail read without looping", async () => {
  let available = true;
  let reads = 0;
  const targets = [{ id: "base", name: "Base" }];
  server.use(
    http.get("/api/v1/workspaces/source/pull-requests/targets", () => {
      reads++;
      return available
        ? HttpResponse.json({ data: targets })
        : HttpResponse.text("Unavailable", { status: 502 });
    }),
  );
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(
    () => useWorkspacePullRequestTargets("source"),
    { wrapper },
  );
  await waitFor(() => expect(result.current.status).toBe("ok"));
  const key = ["workspaces", "pull-request-targets", "source"];
  const readDetail = (id: string) =>
    queryClient.fetchQuery({
      queryKey: ["workspace", id],
      queryFn: async () => ({ id }),
      staleTime: 0,
    });
  available = false;
  await act(async () => {
    await queryClient.invalidateQueries({ queryKey: key });
  });
  expect(reads).toBe(2);
  await act(async () => {
    await readDetail("other");
    queryClient.setQueryData(["workspace", "source"], { id: "source" });
  });
  expect(reads).toBe(2);
  await act(async () => {
    await readDetail("source");
  });
  await waitFor(() => expect(reads).toBe(3));
  await waitFor(() =>
    expect(queryClient.isFetching({ queryKey: key })).toBe(0),
  );
  expect(result.current).toMatchObject({
    status: "ok",
    data: targets,
    refreshError: expect.any(ApiError),
  });
  available = true;
  await act(async () => {
    await readDetail("source");
  });
  await waitFor(() =>
    expect(result.current).toMatchObject({
      status: "ok",
      data: targets,
      refreshError: null,
    }),
  );
  expect(reads).toBe(4);
  await act(async () => {
    await readDetail("source");
  });
  expect(reads).toBe(4);
});

it.each(["initial failure", "403", "404"])(
  "does not automatically retry destinations after %s",
  async (failure) => {
    let failed = failure === "initial failure";
    let reads = 0;
    server.use(
      http.get("/api/v1/workspaces/source/pull-requests/targets", () => {
        reads++;
        return failed
          ? HttpResponse.text("Unavailable", {
              status: failure === "initial failure" ? 502 : Number(failure),
            })
          : HttpResponse.json({ data: [] });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => useWorkspacePullRequestTargets("source"),
      { wrapper },
    );
    await waitFor(() =>
      expect(result.current.status).toBe(failed ? "error" : "ok"),
    );
    if (!failed) {
      failed = true;
      await act(async () => {
        await queryClient.invalidateQueries({
          queryKey: ["workspaces", "pull-request-targets", "source"],
        });
      });
    }
    await waitFor(() => expect(result.current.status).toBe("error"));
    const before = reads;
    await act(async () => {
      await queryClient.fetchQuery({
        queryKey: ["workspace", "source"],
        queryFn: async () => ({ id: "source" }),
      });
    });
    expect(reads).toBe(before);
  },
);

it("keeps an in-flight destination retry and recovers if it fails after the detail read", async () => {
  let reads = 0;
  let finishRetry!: () => void;
  server.use(
    http.get("/api/v1/workspaces/source/pull-requests/targets", async () => {
      reads++;
      if (reads === 3)
        await new Promise<void>((resolve) => {
          finishRetry = resolve;
        });
      return reads === 2 || reads === 3
        ? HttpResponse.text("Unavailable", { status: 502 })
        : HttpResponse.json({ data: [] });
    }),
  );
  const { wrapper, queryClient } = createWrapper();
  const { result } = renderHook(
    () => useWorkspacePullRequestTargets("source"),
    { wrapper },
  );
  await waitFor(() => expect(result.current.status).toBe("ok"));
  const key = ["workspaces", "pull-request-targets", "source"];
  const readDetail = () =>
    queryClient.fetchQuery({
      queryKey: ["workspace", "source"],
      queryFn: async () => ({ id: "source" }),
      staleTime: 0,
    });
  await act(async () => {
    await queryClient.invalidateQueries({ queryKey: key });
  });
  await act(async () => {
    await readDetail();
  });
  await waitFor(() => expect(reads).toBe(3));
  await act(async () => {
    await readDetail();
  });
  expect(reads).toBe(3);
  expect(queryClient.isFetching({ queryKey: key })).toBe(1);
  await act(async () => {
    finishRetry();
  });
  await waitFor(() =>
    expect(queryClient.isFetching({ queryKey: key })).toBe(0),
  );
  expect(result.current).toMatchObject({
    status: "ok",
    refreshError: expect.any(ApiError),
  });
  await act(async () => {
    await readDetail();
  });
  await waitFor(() =>
    expect(result.current).toMatchObject({
      status: "ok",
      data: [],
      refreshError: null,
    }),
  );
  expect(reads).toBe(4);
});
