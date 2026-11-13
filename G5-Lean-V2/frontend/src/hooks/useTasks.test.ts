// =============================================================================
// useTasks.test.ts — Task Hook Integration Tests with MSW
// =============================================================================

import { describe, it, expect } from "vitest";
import React from "react";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import { ApiError } from "../lib/api";
import {
  useWorkspaceTasks,
  useTask,
  useTaskTimeline,
  useBranchCommitHistory,
  useBranchCommitDiff,
  useBranchCommitFileDiff,
} from "./useTasks";
import type { TaskSummary, TaskDetail, TimelineEntry } from "../lib/types";

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

const mockTasks: TaskSummary[] = [
  {
    id: "task-1",
    name: "Uplift auth/validator",
    taskType: "code",
    status: "running",
    workspaceId: "ws-123",
    workspaceName: "Test Workspace",
    specContext: null,
    dependsOn: [],
    filesChanged: [],
    costUsd: 0.75,
    sessionId: null,
    assignedAt: "2024-11-15T10:00:00Z",
    completedAt: null,
  },
  {
    id: "task-2",
    name: "Uplift core/parser",
    taskType: "code",
    status: "succeeded",
    workspaceId: "ws-123",
    workspaceName: "Test Workspace",
    specContext: null,
    dependsOn: [],
    filesChanged: [],
    costUsd: 0.5,
    sessionId: null,
    assignedAt: "2024-11-15T10:00:00Z",
    completedAt: "2024-11-15T10:05:00Z",
  },
];

const mockTaskDetail: TaskDetail = {
  id: "task-1",
  name: "Uplift auth/validator",
  taskType: "code",
  status: "running",
  workspaceId: "ws-123",
  workspaceName: "Test Workspace",
  specContext: null,
  dependsOn: [],
  filesChanged: [],
  costUsd: 0.75,
  sessionId: null,
  assignedAt: "2024-11-15T10:00:00Z",
  completedAt: null,
};

const mockTimeline: TimelineEntry[] = [
  {
    id: "tl-1",
    entryType: "thought",
    content: "Analyzing the auth module...",
    metadata: {},
    createdAt: "2024-11-15T10:01:00Z",
  },
  {
    id: "tl-2",
    entryType: "tool_call",
    content: 'read_file("src/auth/validator.ts")',
    metadata: { tool: "read_file" },
    createdAt: "2024-11-15T10:01:30Z",
  },
];

describe("useTask", () => {
  it("fetches task detail", async () => {
    server.use(
      http.get("/api/v1/tasks/:taskId", () =>
        HttpResponse.json({ data: mockTaskDetail }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useTask("task-1"), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.id).toBe("task-1");
      expect(result.current.data.name).toBe("Uplift auth/validator");
      expect(result.current.data.status).toBe("running");
    }
  });

  it("handles task not found error", async () => {
    server.use(
      http.get("/api/v1/tasks/:taskId", () =>
        HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "Task not found",
              requestId: "req-1",
            },
          },
          { status: 404 },
        ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useTask("nonexistent"), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("error"));

    if (result.current.status === "error") {
      expect(result.current.error).toBeInstanceOf(ApiError);
      expect((result.current.error as ApiError).code).toBe("NOT_FOUND");
    }
  });
});

describe("useTaskTimeline", () => {
  it("fetches task timeline with cursor pagination", async () => {
    server.use(
      http.get("/api/v1/tasks/:taskId/timeline", () =>
        HttpResponse.json({
          data: {
            entries: mockTimeline,
            cursor: "cursor-abc",
            hasMore: true,
          },
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useTaskTimeline("task-1"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.timelineState.status).toBe("ok"));

    if (result.current.timelineState.status === "ok") {
      expect(result.current.timelineState.data.items).toHaveLength(2);
      expect(result.current.timelineState.data.hasMore).toBe(true);
      expect(result.current.timelineState.data.cursor).toBe("cursor-abc");
    }
  });
});

describe("Source Control history hooks", () => {
  it("is demand-loaded and requests exactly 10 commits per page without duplicates", async () => {
    const firstSha = "a".repeat(40);
    const secondSha = "b".repeat(40);
    const thirdSha = "c".repeat(40);
    const requests: Array<{ limit: string | null; cursor: string | null }> = [];
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/commits/history",
        ({ request }) => {
          const query = new URL(request.url).searchParams;
          const cursor = query.get("cursor");
          requests.push({ limit: query.get("limit"), cursor });
          const entry = (sha: string, summary: string) => ({
            sha,
            shortSha: sha.slice(0, 7),
            summary,
            authorName: "Ada Lovelace",
            committedAt: "2026-07-15T12:00:00Z",
            parentCount: 1,
          });
          return HttpResponse.json({
            data: cursor
              ? {
                  commits: [
                    entry(secondSha, "duplicate"),
                    entry(thirdSha, "third"),
                  ],
                  nextCursor: null,
                }
              : {
                  commits: [
                    entry(firstSha, "first"),
                    entry(secondSha, "second"),
                  ],
                  nextCursor: thirdSha,
                },
          });
        },
      ),
    );
    const { wrapper } = createWrapper();
    const { result, rerender } = renderHook(
      ({ enabled }) =>
        useBranchCommitHistory("workspace-1", "repo-code", enabled),
      { wrapper, initialProps: { enabled: false } },
    );

    expect(requests).toHaveLength(0);
    rerender({ enabled: true });
    await waitFor(() => expect(result.current.commits).toHaveLength(2));
    expect(requests).toEqual([{ limit: "10", cursor: null }]);

    await act(async () => {
      await result.current.fetchNextPage();
    });
    await waitFor(() =>
      expect(result.current.commits.map((commit) => commit.sha)).toEqual([
        firstSha,
        secondSha,
        thirdSha,
      ]),
    );
    expect(requests[1]).toEqual({ limit: "10", cursor: thirdSha });
  });

  it("restarts stale pagination and keeps commit/file keys scoped by repository, branch, SHA, and path", async () => {
    let stale = true;
    const sha = "a".repeat(40);
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/commits/history",
        () => {
          if (stale) {
            stale = false;
            return HttpResponse.json(
              {
                error: {
                  code: "STALE_HISTORY",
                  message: "history changed",
                  requestId: "request-1",
                },
              },
              { status: 409 },
            );
          }
          return HttpResponse.json({
            data: { commits: [], nextCursor: null },
          });
        },
      ),
      http.get(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/commits/:sha/diff",
        () =>
          HttpResponse.json({
            data: {
              commit: {
                sha,
                shortSha: sha.slice(0, 7),
                summary: "commit",
                authorName: "Ada",
                committedAt: "2026-07-15T12:00:00Z",
                parentCount: 0,
              },
              comparisonBase: { kind: "emptyTree" },
              firstParentDiff: false,
              files: [],
              totalAdditions: 0,
              totalDeletions: 0,
            },
          }),
      ),
      http.get(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/commits/:sha/diff/files/*",
        () =>
          HttpResponse.json({
            data: {
              path: "src/a file.ts",
              status: "text",
              additions: 0,
              deletions: 0,
              hunks: [],
            },
          }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const history = renderHook(
      () => useBranchCommitHistory("workspace-1", "repo-code", true),
      { wrapper },
    );
    renderHook(() => useBranchCommitDiff("workspace-1", "repo-code", sha), {
      wrapper,
    });
    renderHook(
      () =>
        useBranchCommitFileDiff(
          "workspace-1",
          "repo-code",
          sha,
          "src/a file.ts",
        ),
      { wrapper },
    );

    await waitFor(() =>
      expect(history.result.current.error).toBeInstanceOf(ApiError),
    );
    await act(async () => {
      await history.result.current.restart();
    });
    await waitFor(() => expect(history.result.current.error).toBeNull());

    expect(
      queryClient.getQueryState([
        "source-control-history",
        "repo-code",
        "workspace-1",
      ]),
    ).toBeDefined();
    expect(
      queryClient.getQueryState([
        "source-control-commit-diff",
        "repo-code",
        "workspace-1",
        sha,
      ]),
    ).toBeDefined();
    expect(
      queryClient.getQueryState([
        "source-control-commit-file-diff",
        "repo-code",
        "workspace-1",
        sha,
        "src/a file.ts",
      ]),
    ).toBeDefined();
  });
});

describe("workspace task pagination", () => {
  it("keeps existing tasks on page failure, retries, and deduplicates page boundaries", async () => {
    let attempts = 0;
    server.use(
      http.get("/api/v1/workspaces/ws/tasks", ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        if (cursor && ++attempts === 1)
          return HttpResponse.json(
            { error: { code: "TEMPORARY", message: "Retry" } },
            { status: 503 },
          );
        return HttpResponse.json({
          data: cursor
            ? {
                tasks: [{ id: "new" }, { id: "old" }],
                cursor: null,
                hasMore: false,
              }
            : { tasks: [{ id: "new" }], cursor: "older", hasMore: true },
        });
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspaceTasks("ws"), { wrapper });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    await act(() => result.current.fetchNextPage());
    expect(result.current.status).toBe("ok");
    await waitFor(() => expect(result.current.paginationError).toBeTruthy());
    if (result.current.status === "ok")
      expect(result.current.data.map((t) => t.id)).toEqual(["new"]);
    await act(() => result.current.fetchNextPage());
    await waitFor(() => expect(result.current.hasNextPage).toBe(false));
    if (result.current.status === "ok")
      expect(result.current.data.map((t) => t.id)).toEqual(["new", "old"]);
  });
  it("loads an older page when clicked during a background refresh", async () => {
    let firstReads = 0,
      release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/ws/tasks", async ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        if (!cursor && ++firstReads === 2) await gate;
        return HttpResponse.json({
          data: cursor
            ? { tasks: [{ id: "old" }], cursor: null, hasMore: false }
            : { tasks: [{ id: "new" }], cursor: "older", hasMore: true },
        });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useWorkspaceTasks("ws"), { wrapper });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    let refresh!: Promise<void>;
    act(() => {
      refresh = queryClient.invalidateQueries({
        queryKey: ["workspace-tasks", "ws"],
      });
    });
    await waitFor(() => expect(firstReads).toBe(2));
    await act(() => result.current.fetchNextPage());
    await waitFor(() => expect(result.current.total).toBe(2));
    await act(async () => {
      release();
      await refresh;
    });
    if (result.current.status === "ok")
      expect(result.current.data.map((t) => t.id)).toEqual(["new", "old"]);
  });
});
