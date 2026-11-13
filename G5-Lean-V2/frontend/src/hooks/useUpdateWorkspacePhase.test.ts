// =============================================================================
// useUpdateWorkspacePhase — optimistic phase move bound to its workspace
// =============================================================================
// Workbench g5.workbench.phases.state: the chevron reflects a move at once and
// rolls back on refusal. The rollback and the refetch must target the
// workspace the move was issued for, even when the hook has since re-rendered
// for another workspace (A → B navigation while A's PATCH is in flight).
// =============================================================================

import React from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import {
  QueryClient,
  QueryClientProvider,
  onlineManager,
} from "@tanstack/react-query";
import { delay, http, HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";

import type { WorkspaceDetail } from "../lib/types";
import { mockWorkspaceDetails } from "../test/fixtures";
import { server } from "../test/msw/server";
import { useUpdateWorkspacePhase } from "./useWorkspace";

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

const baseDetail = Object.values(mockWorkspaceDetails)[0];

function detail(id: string, phase: WorkspaceDetail["phase"]): WorkspaceDetail {
  return { ...baseDetail, id, name: id, isBase: false, phase };
}

const refusal = HttpResponse.json(
  {
    error: {
      code: "FORBIDDEN",
      message: "Only the owner moves the phase",
      details: {},
    },
  },
  { status: 403 },
);

describe("useUpdateWorkspacePhase", () => {
  it("applies the move optimistically and keeps it once the PATCH succeeds", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["workspace", "ws-a"], detail("ws-a", "be"));
    server.use(
      http.patch("/api/v1/workspaces/ws-a", async ({ request }) => {
        const body = (await request.json()) as { phase: string };
        return HttpResponse.json({
          data: { ...detail("ws-a", body.phase as "se"), phase: body.phase },
        });
      }),
    );

    const { result } = renderHook(() => useUpdateWorkspacePhase("ws-a"), {
      wrapper,
    });
    act(() => {
      result.current.mutate("se");
    });
    // onMutate awaits cancelQueries before writing the optimistic phase.
    await waitFor(() =>
      expect(
        queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-a"])?.phase,
      ).toBe("se"),
    );

    await waitFor(() => expect(result.current.data?.phase).toBe("se"));
    expect(result.current.error).toBeNull();
    expect(
      queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-a"])?.phase,
    ).toBe("se");
  });

  it("rolls back only the phase of the workspace the move was issued for after navigating away", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["workspace", "ws-a"], detail("ws-a", "be"));
    queryClient.setQueryData(["workspace", "ws-b"], detail("ws-b", "ie"));
    server.use(
      http.patch("/api/v1/workspaces/ws-a", async () => {
        await delay(50);
        return refusal.clone();
      }),
    );

    const { result, rerender } = renderHook(
      ({ workspaceId }: { workspaceId: string }) =>
        useUpdateWorkspacePhase(workspaceId),
      { wrapper, initialProps: { workspaceId: "ws-a" } },
    );
    act(() => {
      result.current.mutate("se");
    });
    await waitFor(() =>
      expect(
        queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-a"])?.phase,
      ).toBe("se"),
    );

    // The user opens workspace B while A's PATCH is still pending; a refetch
    // of A lands meanwhile with a newer unrelated field.
    rerender({ workspaceId: "ws-b" });
    queryClient.setQueryData<WorkspaceDetail>(["workspace", "ws-a"], (cur) =>
      cur ? { ...cur, description: "refreshed meanwhile" } : cur,
    );

    // The refusal belongs to A's move: B's instance stays clean, and an
    // instance for A (its chevrons, were the user to return) reports it.
    const aAgain = renderHook(() => useUpdateWorkspacePhase("ws-a"), {
      wrapper,
    });
    await waitFor(() => expect(aAgain.result.current.error).not.toBeNull());
    expect(aAgain.result.current.error?.message).toContain(
      "Only the owner moves the phase",
    );
    expect(result.current.error).toBeNull();
    expect(result.current.isLoading).toBe(false);

    const a = queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-a"]);
    expect(a?.phase).toBe("be");
    expect(a?.description).toBe("refreshed meanwhile");
    const b = queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-b"]);
    expect(b).toEqual(detail("ws-b", "ie"));
  });

  afterEach(() => {
    onlineManager.setOnline(true);
  });

  it("resumes a paused move against the workspace it was issued for, not the current one", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["workspace", "ws-a"], detail("ws-a", "be"));
    queryClient.setQueryData(["workspace", "ws-b"], detail("ws-b", "ie"));
    const patched: string[] = [];
    server.use(
      http.patch("/api/v1/workspaces/:id", async ({ params, request }) => {
        const body = (await request.json()) as { phase: "se" };
        patched.push(String(params.id));
        return HttpResponse.json({
          data: detail(String(params.id), body.phase),
        });
      }),
    );

    // Offline: the mutation is paused before `mutationFn` runs.
    onlineManager.setOnline(false);
    const { result, rerender } = renderHook(
      ({ workspaceId }: { workspaceId: string }) =>
        useUpdateWorkspacePhase(workspaceId),
      { wrapper, initialProps: { workspaceId: "ws-a" } },
    );
    act(() => {
      result.current.mutate("se");
    });
    await waitFor(() =>
      expect(
        queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-a"])?.phase,
      ).toBe("se"),
    );
    expect(patched).toEqual([]);

    // The owner opens workspace B, then the connection returns and React
    // Query resumes the paused mutation with the LATEST mutationFn.
    rerender({ workspaceId: "ws-b" });
    act(() => {
      onlineManager.setOnline(true);
    });

    await waitFor(() => expect(patched).toEqual(["ws-a"]));
    expect(
      queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-b"])?.phase,
    ).toBe("ie");
  });

  // A move is filed under the workspace's shared mutation key: the page the
  // user lands on next (its own hook instance) sees it pending and, on a
  // refusal, reports the error after the page that started it is gone.
  it("shares pending and error state with a later instance for the same workspace", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["workspace", "ws-a"], detail("ws-a", "be"));
    server.use(
      http.patch("/api/v1/workspaces/ws-a", async () => {
        await delay(80);
        return refusal.clone();
      }),
    );

    const starter = renderHook(() => useUpdateWorkspacePhase("ws-a"), {
      wrapper,
    });
    act(() => {
      starter.result.current.mutate("se");
    });
    await waitFor(() => expect(starter.result.current.isLoading).toBe(true));
    starter.unmount();

    const landing = renderHook(() => useUpdateWorkspacePhase("ws-a"), {
      wrapper,
    });
    expect(landing.result.current.isLoading).toBe(true);
    expect(landing.result.current.error).toBeNull();

    await waitFor(() => expect(landing.result.current.error).not.toBeNull());
    expect(landing.result.current.isLoading).toBe(false);
    expect(landing.result.current.error?.message).toContain(
      "Only the owner moves the phase",
    );
    expect(
      queryClient.getQueryData<WorkspaceDetail>(["workspace", "ws-a"])?.phase,
    ).toBe("be");

    // Another workspace's instance is untouched by ws-a's move.
    const other = renderHook(() => useUpdateWorkspacePhase("ws-b"), {
      wrapper,
    });
    expect(other.result.current.isLoading).toBe(false);
    expect(other.result.current.error).toBeNull();
  });

  it("refuses a second move while one is in flight so writes never overlap", async () => {
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(["workspace", "ws-a"], detail("ws-a", "be"));
    const patched: string[] = [];
    server.use(
      http.patch("/api/v1/workspaces/ws-a", async ({ request }) => {
        const body = (await request.json()) as { phase: "se" | "ie" };
        patched.push(body.phase);
        await delay(80);
        return HttpResponse.json({ data: detail("ws-a", body.phase) });
      }),
    );

    const { result } = renderHook(() => useUpdateWorkspacePhase("ws-a"), {
      wrapper,
    });
    act(() => {
      result.current.mutate("se");
    });
    await waitFor(() => expect(result.current.isLoading).toBe(true));
    act(() => {
      result.current.mutate("ie");
    });
    await expect(result.current.mutateAsync("ie")).rejects.toThrow(
      "already in flight",
    );

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(patched).toEqual(["se"]);
    expect(result.current.data?.phase).toBe("se");

    // Settled: the next move is accepted.
    act(() => {
      result.current.mutate("ie");
    });
    await waitFor(() => expect(patched).toEqual(["se", "ie"]));
  });
});
