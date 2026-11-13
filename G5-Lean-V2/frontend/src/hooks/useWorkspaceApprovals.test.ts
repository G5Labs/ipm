import {
  QueryClient,
  QueryClientProvider,
  focusManager,
} from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { server } from "../test/msw/server";
import type {
  OrgApprovalDefinitionDTO,
  WorkspaceApprovalsStateDTO,
} from "../lib/types";
import {
  approvalStateRefetchInterval,
  useCancelApprovalReview,
  useRequestApprovals,
  useWorkspaceApprovals,
} from "./useWorkspaceApprovals";

function createWrapper(queryOverrides?: { staleTime?: number }) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        staleTime: queryOverrides?.staleTime ?? Infinity,
        gcTime: Infinity,
      },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);

  return { wrapper, queryClient };
}

function pollingState(
  agentState: "running" | "waiting" | "succeeded",
): WorkspaceApprovalsStateDTO {
  const definition: OrgApprovalDefinitionDTO = {
    id: "def-poll",
    latestVersion: 1,
    name: "Security",
    description: null,
    humans: [],
    agentType: "simple",
    agentPrompt: "Review the change.",
    invalidatesPrevious: false,
    createdBy: { userId: "user-1", displayName: "Alex" },
    createdAt: "2026-08-27T09:00:00Z",
    firstCreatedBy: { userId: "user-1", displayName: "Alex" },
    firstCreatedAt: "2026-08-27T09:00:00Z",
  };
  const request = {
    requestId: "request-poll",
    actorType: "agent" as const,
    actorUserId: null,
    actor: null,
    requestedBy: { userId: "user-2", displayName: "Jordan" },
    state: "pending" as const,
    definitionVersion: 1,
    definitionName: definition.name,
    requestedAt: "2026-08-27T10:00:00Z",
    closedAt: null,
    agentReviews: [
      {
        sessionId: null,
        taskId: "task-poll",
        state: agentState,
        terminalReason: null,

        createdAt: "2026-08-27T10:00:00Z",
        startedAt: "2026-08-27T10:01:00Z",

        updatedAt: "2026-08-27T10:04:00Z",
      },
    ],
  };
  return {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "target",
      targetWorkspaceName: "Target",
      requirements: [
        {
          definition,
          state: "pending",
          approvers: [],
          callerCanDecide: false,
          requests: [request],
          decisionLog: [],
        },
      ],
      history: [
        {
          definitionId: definition.id,
          definitionName: definition.name,
          requests: [request],
          decisionLog: [],
        },
      ],
    },
  };
}

describe("useRequestApprovals", () => {
  it("preserves per-row success when another bulk request fails", async () => {
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/approvals/requests", () =>
        HttpResponse.json(
          {
            data: {
              requests: [
                {
                  requestId: "human-request",
                  actorUserId: "user-1",
                  status: "pending",
                  reused: false,
                },
              ],
            },
          },
          { status: 201 },
        ),
      ),
      http.post("/api/v1/workspaces/:workspaceId/approvals/reviews", () =>
        HttpResponse.json(
          { error: { code: "CONFLICT", message: "agent unavailable" } },
          { status: 409 },
        ),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useRequestApprovals("ws-1"), {
      wrapper,
    });

    const results = await act(() =>
      result.current.mutateAsync([
        {
          key: "human",
          label: "Security — Alex",
          kind: "human",
          definitionId: "def-1",
          definitionVersion: 1,
          phase: "ie",
          actorUserId: "user-1",
        },
        {
          key: "agent",
          label: "Security — Security agent",
          kind: "agent",
          definitionId: "def-1",
          definitionVersion: 1,
          phase: "ie",
        },
      ]),
    );

    expect(results.map((entry) => entry.status)).toEqual([
      "fulfilled",
      "rejected",
    ]);
    expect(results[1].message).toContain("agent unavailable");
  });

  it("keeps a completed batch pending until its approvals refresh can seed a fresh selection", async () => {
    const initial = pollingState("succeeded");
    initial.outgoing!.requirements[0].requests = [];
    initial.outgoing!.history = [];
    let admitted = false;
    let refreshStarted = false;
    let releaseRefresh!: () => void;
    const refresh = new Promise<void>((resolve) => {
      releaseRefresh = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/approvals", async () => {
        if (admitted) {
          refreshStarted = true;
          await refresh;
        }
        return HttpResponse.json({
          data: admitted ? pollingState("running") : initial,
        });
      }),
      http.post("/api/v1/workspaces/:workspaceId/approvals/reviews", () => {
        admitted = true;
        return HttpResponse.json(
          {
            data: {
              requestId: "request-poll",
              reviewId: "review-poll",
              reused: false,
            },
          },
          { status: 201 },
        );
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        state: useWorkspaceApprovals("ws-1"),
        request: useRequestApprovals("ws-1"),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.state.status).toBe("ok"));
    let receipt!: ReturnType<typeof result.current.request.mutateAsync>;
    act(() => {
      receipt = result.current.request.mutateAsync([
        {
          key: "agent",
          label: "Security agent",
          kind: "agent",
          definitionId: "def-poll",
          definitionVersion: 1,
          phase: "ie",
        },
      ]);
    });
    try {
      await waitFor(() => expect(refreshStarted).toBe(true));
      // Reopening after a completed receipt must not freeze the pre-request
      // roster while this authoritative read is still in flight.
      expect(result.current.request.isLoading).toBe(true);
    } finally {
      releaseRefresh();
      await act(() => receipt);
    }
    await waitFor(() => expect(result.current.request.isLoading).toBe(false));
    expect(result.current.state.status).toBe("ok");
    if (result.current.state.status === "ok") {
      expect(
        result.current.state.data.outgoing!.requirements[0].requests![0]
          .requestId,
      ).toBe("request-poll");
    }
  });

  it("invalidates the approvals cache exactly once per bulk settlement", async () => {
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/approvals/requests", () =>
        HttpResponse.json(
          {
            data: {
              requests: [
                {
                  requestId: "human-request",
                  actorUserId: "user-1",
                  status: "pending",
                  reused: false,
                },
              ],
            },
          },
          { status: 201 },
        ),
      ),
      http.post("/api/v1/workspaces/:workspaceId/approvals/reviews", () =>
        HttpResponse.json(
          {
            data: {
              requestId: "agent-request",
              status: "pending",
              reused: false,
            },
          },
          { status: 201 },
        ),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    const { result } = renderHook(() => useRequestApprovals("ws-1"), {
      wrapper,
    });

    await act(() =>
      result.current.mutateAsync([
        {
          key: "human",
          label: "Security — Alex",
          kind: "human",
          definitionId: "def-1",
          definitionVersion: 1,
          phase: "ie",
          actorUserId: "user-1",
        },
        {
          key: "agent",
          label: "Security — Security agent",
          kind: "agent",
          definitionId: "def-1",
          definitionVersion: 1,
          phase: "ie",
        },
      ]),
    );

    const keys = () =>
      invalidateSpy.mock.calls.map((call) => JSON.stringify(call[0]?.queryKey));
    await waitFor(() => {
      expect(
        keys().filter(
          (key) => key === JSON.stringify(["workspace-approvals", "ws-1"]),
        ),
      ).toHaveLength(1);
      expect(
        keys().filter(
          (key) => key === JSON.stringify(["workspace-tasks", "ws-1"]),
        ),
      ).toHaveLength(1);
    });
    expect(keys()).toContain(
      JSON.stringify(["workspace-pull-requests", "ws-1"]),
    );
    expect(keys()).toContain(JSON.stringify(["workspace-activity", "ws-1"]));
    expect(invalidateSpy).toHaveBeenCalledTimes(5);
  });
});

describe("approvalStateRefetchInterval", () => {
  const definition = (id: string): OrgApprovalDefinitionDTO => ({
    id,
    latestVersion: 1,
    name: "Security",
    description: null,
    humans: [],
    agentType: null,
    agentPrompt: null,
    invalidatesPrevious: false,
    createdBy: { userId: "user-1", displayName: "Alex" },
    createdAt: "2026-08-27T09:00:00Z",
    firstCreatedBy: { userId: "user-1", displayName: "Alex" },
    firstCreatedAt: "2026-08-27T09:00:00Z",
  });

  const stateWithRequest = (
    actorType: "human" | "agent",
    agentState?:
      | "queued"
      | "running"
      | "running"
      | "waiting"
      | "succeeded"
      | "failed",
    opts?: { retired?: boolean },
  ): WorkspaceApprovalsStateDTO => ({
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "target",
      targetWorkspaceName: "Target",
      // A retired definition is absent from `requirements`; its pending
      // requests survive only in `history`.
      requirements: opts?.retired
        ? []
        : [
            {
              definition: definition("def-1"),
              state: "pending",
              approvers: [],
              callerCanDecide: false,
              decisionLog: [],
            },
          ],
      history: [
        {
          definitionId: "def-1",
          definitionName: "Security",
          requests: [
            {
              requestId: "request-1",
              actorType,
              actorUserId: actorType === "human" ? "user-1" : null,
              actor:
                actorType === "human"
                  ? { userId: "user-1", displayName: "Alex" }
                  : null,
              requestedBy: { userId: "user-2", displayName: "Jordan" },
              state: "pending",
              definitionVersion: 1,
              definitionName: "Security",
              requestedAt: "2026-08-27T10:00:00Z",
              closedAt: null,
              agentReviews: agentState
                ? [
                    {
                      sessionId: null,
                      taskId: null,
                      state: agentState,
                      terminalReason:
                        agentState === "failed" ? "provider_timeout" : null,

                      createdAt: "2026-08-27T10:00:00Z",
                      startedAt: "2026-08-27T10:01:00Z",

                      updatedAt: "2026-08-27T10:02:00Z",
                    },
                  ]
                : [],
            },
          ],
          decisionLog: [],
        },
      ],
    },
  });

  it("polls active agents quickly, humans slowly, and stops after agent failure", () => {
    expect(
      approvalStateRefetchInterval(stateWithRequest("agent", "running")),
    ).toBe(2_500);
    expect(approvalStateRefetchInterval(stateWithRequest("human"))).toBe(
      15_000,
    );
    expect(
      approvalStateRefetchInterval(stateWithRequest("agent", "failed")),
    ).toBe(false);
  });

  it("polls fast while an agent slot awaits its first run or is executing", () => {
    // Pending agent request with no runs yet: the slot awaits assignment.
    expect(approvalStateRefetchInterval(stateWithRequest("agent"))).toBe(2_500);
    expect(
      approvalStateRefetchInterval(stateWithRequest("agent", "queued")),
    ).toBe(2_500);
    expect(
      approvalStateRefetchInterval(stateWithRequest("agent", "running")),
    ).toBe(2_500);
  });

  it("polls a waiting review while it needs input", () => {
    expect(
      approvalStateRefetchInterval(stateWithRequest("agent", "waiting")),
    ).toBe(2_500);
  });

  it("keeps active formal reviews fast while cleanup is pending", () => {
    const state = pollingState("running");
    const request = state.outgoing!.requirements[0].requests![0];
    request.agentReviews[0].cleanupPending = true;
    expect(approvalStateRefetchInterval(state)).toBe(2_500);
    request.agentReviews[0].state = "succeeded";
    request.state = "settled";
    expect(approvalStateRefetchInterval(state)).toBe(15_000);
    request.agentReviews[0].cleanupPending = false;
    expect(approvalStateRefetchInterval(state)).toBe(false);
  });

  it("stops polling a completed review without treating it as a decision", () => {
    expect(
      approvalStateRefetchInterval(stateWithRequest("agent", "succeeded")),
    ).toBe(false);
  });

  it.each([
    ["human", undefined],
    ["agent", "running"],
  ] as const)(
    "does not keep polling for a pending %s request on a retired definition",
    (actorType, agentState) => {
      expect(
        approvalStateRefetchInterval(
          stateWithRequest(actorType, agentState, { retired: true }),
        ),
      ).toBe(false);
    },
  );
});

describe("useWorkspaceApprovals", () => {
  afterEach(() => {
    // Restore React Query's real focus detection after forcing focus states.
    focusManager.setFocused(undefined);
    vi.useRealTimers();
  });

  it("returns an ambiguous request failure while its durable-state refresh is still pending", async () => {
    let gets = 0;
    let admitted = false;
    let refreshStarted = false;
    let releaseRefresh!: () => void;
    const refresh = new Promise<void>((resolve) => {
      releaseRefresh = resolve;
    });
    let durableState: WorkspaceApprovalsStateDTO = {
      requiredInto: [],
      attachmentHistory: [],
      outgoing: null,
    };
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/approvals", async () => {
        gets += 1;
        if (admitted) {
          refreshStarted = true;
          await refresh;
        }
        return HttpResponse.json({ data: durableState });
      }),
      http.post("/api/v1/workspaces/:workspaceId/approvals/requests", () => {
        admitted = true;
        durableState = pollingState("succeeded");
        return HttpResponse.error();
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        state: useWorkspaceApprovals("ws-1"),
        request: useRequestApprovals("ws-1"),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.state.status).toBe("ok"));
    expect(gets).toBe(1);

    let receipt!: ReturnType<typeof result.current.request.mutateAsync>;
    act(() => {
      receipt = result.current.request.mutateAsync([
        {
          key: "human",
          label: "Security — Alex",
          kind: "human",
          definitionId: "def-poll",
          definitionVersion: 1,
          phase: "ie",
          actorUserId: "user-1",
        },
      ]);
    });
    try {
      await waitFor(() => expect(refreshStarted).toBe(true));
      // The dialog must receive the failed receipt and enable its stable
      // retry even when the separate roster read has not answered yet.
      await waitFor(() => expect(result.current.request.isLoading).toBe(false));
      const results = await receipt;
      expect(results[0].status).toBe("rejected");
    } finally {
      releaseRefresh();
      await act(() => receipt);
    }

    await waitFor(() => {
      expect(gets).toBeGreaterThanOrEqual(2);
      expect(result.current.state.status).toBe("ok");
      if (result.current.state.status === "ok") {
        expect(
          result.current.state.data.outgoing?.history?.[0].requests[0]
            .requestId,
        ).toBe("request-poll");
      }
    });
  });

  it("polls fast through projection settlement, then stops", async () => {
    const realSetTimeout = globalThis.setTimeout;
    vi.useFakeTimers();
    const states = [
      pollingState("running"),
      pollingState("waiting"),
      pollingState("succeeded"),
    ];
    let gets = 0;
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/approvals", () => {
        const state = states[Math.min(gets + 1, states.length - 1)];
        gets += 1;
        return HttpResponse.json({ data: state });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    // Seed the active state so fake timers own the very first scheduled poll;
    // the first GET is therefore the 2.5s transition, not initial hydration.
    queryClient.setQueryData(["workspace-approvals", "ws-1"], states[0]);
    const { result } = renderHook(() => useWorkspaceApprovals("ws-1"), {
      wrapper,
    });
    expect(result.current.status).toBe("ok");
    expect(gets).toBe(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_499);
    });
    expect(gets).toBe(0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
      // Flush the fetch and React Query notification without advancing the
      // fake polling clock into the next tier.
      for (let tick = 0; tick < 20; tick += 1) {
        await new Promise<void>((resolve) => realSetTimeout(resolve, 0));
        await vi.advanceTimersByTimeAsync(1);
        if (
          result.current.status === "ok" &&
          result.current.data.outgoing?.history?.[0].requests[0].agentReviews[0]
            .state === "waiting"
        ) {
          break;
        }
      }
    });
    expect(gets).toBe(1);
    expect(result.current.status).toBe("ok");
    if (result.current.status === "ok") {
      expect(
        result.current.data.outgoing?.history?.[0].requests[0].agentReviews[0]
          .state,
      ).toBe("waiting");
    }

    await act(async () => {
      // The previous notification flush may consume a few milliseconds of the
      // fake clock, so cross the fast-tier boundary with a small cushion.
      await vi.advanceTimersByTimeAsync(2_600);
      for (let tick = 0; tick < 20; tick += 1) {
        await new Promise<void>((resolve) => realSetTimeout(resolve, 0));
        await vi.advanceTimersByTimeAsync(1);
        if (
          result.current.status === "ok" &&
          result.current.data.outgoing?.history?.[0].requests[0].agentReviews[0]
            .state === "succeeded"
        ) {
          break;
        }
      }
    });
    expect(gets).toBe(2);
    expect(result.current.status).toBe("ok");
    if (result.current.status === "ok") {
      expect(
        result.current.data.outgoing?.history?.[0].requests[0].agentReviews[0]
          .state,
      ).toBe("succeeded");
    }

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(gets).toBe(2);
  });

  it("refetches the approvals state on window focus", async () => {
    let gets = 0;
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/approvals", () => {
        gets += 1;
        return HttpResponse.json({
          data: { requiredInto: [], attachmentHistory: [], outgoing: null },
        });
      }),
    );
    // staleTime 0 so the refocus finds the data stale; the query opts into
    // `refetchOnWindowFocus: true` against the app-wide `false` default.
    const { wrapper } = createWrapper({ staleTime: 0 });
    focusManager.setFocused(false);
    const { result } = renderHook(() => useWorkspaceApprovals("ws-1"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(gets).toBe(1);

    act(() => {
      focusManager.setFocused(true);
    });
    await waitFor(() => expect(gets).toBe(2));
  });
});

describe("formal request cancellation", () => {
  it("cancels the parent request while preserving the legacy task route", async () => {
    const requests: string[] = [];
    const tasks: string[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/approvals/requests/:requestId/cancel",
        ({ params }) => {
          requests.push(String(params.requestId));
          return HttpResponse.json({ data: {} });
        },
      ),
      http.post("/api/v1/tasks/:taskId/cancel", ({ params }) => {
        tasks.push(String(params.taskId));
        return HttpResponse.json({ data: {} });
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useCancelApprovalReview("workspace"), {
      wrapper,
    });
    await act(() =>
      result.current.mutateAsync({
        taskId: "formal-task",
        requestId: "parent-request",
      }),
    );
    expect(requests).toEqual(["parent-request"]);
    expect(tasks).toEqual([]);
    await act(() => result.current.mutateAsync({ taskId: "legacy-task" }));
    expect(tasks).toEqual(["legacy-task"]);
  });
});
