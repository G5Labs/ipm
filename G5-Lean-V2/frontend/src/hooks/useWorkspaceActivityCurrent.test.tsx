// =============================================================================
// useWorkspaceActivityCurrent — ongoing work is polled and, when an entry
// leaves the set, the record is invalidated (ui g5.ui.activity-current)
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import {
  externalIssueKeys,
  useWorkspaceActivityCurrent,
} from "./useExternalIssues";

function wrapperFor(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

const entry = (key: string) => ({
  key,
  at: "2026-01-07T10:00:00Z",
  source: "workspace",
  kind: "task",
  taskId: key,
  name: key,
  taskKind: "command",
  status: "running",
  reason: null,
});

describe("useWorkspaceActivityCurrent", () => {
  it("invalidates the record when an entry leaves the set, not when one joins", async () => {
    const sets = [["task:a"], ["task:a", "task:b"], ["task:b"]];
    let call = 0;
    server.use(
      http.get("/api/v1/workspaces/:id/activity/current", () => {
        const keys = sets[Math.min(call, sets.length - 1)];
        call += 1;
        return HttpResponse.json({ data: { entries: keys.map(entry) } });
      }),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    // A loaded record whose staleness the hook controls.
    queryClient.setQueryData(externalIssueKeys.activity("ws-1"), {
      pages: [],
      pageParams: [],
    });
    const recordState = () =>
      queryClient.getQueryState(externalIssueKeys.activity("ws-1"));

    const { result } = renderHook(() => useWorkspaceActivityCurrent("ws-1"), {
      wrapper: wrapperFor(queryClient),
    });
    await waitFor(() =>
      expect(result.current.data?.entries.map((e) => e.key)).toEqual([
        "task:a",
      ]),
    );
    expect(recordState()?.isInvalidated).toBe(false);

    await result.current.refetch();
    await waitFor(() =>
      expect(result.current.data?.entries.map((e) => e.key)).toEqual([
        "task:a",
        "task:b",
      ]),
    );
    // Joining is a new row at the top; nothing in the record changed.
    expect(recordState()?.isInvalidated).toBe(false);

    await result.current.refetch();
    await waitFor(() =>
      expect(result.current.data?.entries.map((e) => e.key)).toEqual([
        "task:b",
      ]),
    );
    // task:a finished: it now belongs in the record at its final position.
    await waitFor(() => expect(recordState()?.isInvalidated).toBe(true));
  });

  it("does not read a workspace switch as entries leaving", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id/activity/current", ({ params }) =>
        HttpResponse.json({
          data: { entries: [entry(`task:${params.id}`)] },
        }),
      ),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    queryClient.setQueryData(externalIssueKeys.activity("ws-2"), {
      pages: [],
      pageParams: [],
    });
    const { result, rerender } = renderHook(
      ({ id }: { id: string }) => useWorkspaceActivityCurrent(id),
      { wrapper: wrapperFor(queryClient), initialProps: { id: "ws-1" } },
    );
    await waitFor(() =>
      expect(result.current.data?.entries[0]?.key).toBe("task:ws-1"),
    );
    rerender({ id: "ws-2" });
    await waitFor(() =>
      expect(result.current.data?.entries[0]?.key).toBe("task:ws-2"),
    );
    expect(
      queryClient.getQueryState(externalIssueKeys.activity("ws-2"))
        ?.isInvalidated,
    ).toBe(false);
  });
});
