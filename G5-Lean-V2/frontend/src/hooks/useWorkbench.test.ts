// =============================================================================
// useWorkbench.test.ts — workbench configuration and templated starts (MSW)
// =============================================================================
// Workbench g5.workbench.actions / g5.workbench.persona: the start payload
// carries the persona, action, context and pinned revision with a request
// key; a stale revision (409 WORKBENCH_CONFIG_STALE) drops the cached
// configuration and surfaces the retry message; follow view forks with the
// persona.

import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import type { AgentLaunchSelectionV1 } from "../lib/types";
import {
  WORKBENCH_CONFIG_STALE_MESSAGE,
  useFollowView,
  useStartWorkbenchAction,
  useWorkbenchConfig,
  workbenchConfigKey,
} from "./useWorkbench";

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

const agentLaunch = {
  schemaVersion: 1,
  preferences: { schemaVersion: 1 },
  confirmedResolutionId: "res-1",
} as unknown as AgentLaunchSelectionV1;

const startedSession = {
  id: "session-wb",
  workspaceId: "ws-1",
  scopeType: "workspace",
  scopeRef: "ws-1",
  model: null,
  agentProvider: "claude_code",
  title: "Build mock",
  summary: null,
  tags: [],
  messageCount: 0,
  totalInputTokens: 0,
  totalOutputTokens: 0,
  totalCostUsd: 0,
  forkedFromId: null,
  linkedCommits: [],
  createdAt: "2026-10-06T00:00:00Z",
  updatedAt: "2026-10-06T00:00:00Z",
  canSend: true,
  persona: "be",
};

describe("useWorkbenchConfig", () => {
  it("reads a malformed payload as the error state, never as a configuration (g5.ui.workbench-config)", async () => {
    server.use(
      http.get("/api/v1/workspaces/ws-bad/workbench", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkbenchConfig("ws-bad", true), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(
      result.current.status === "error" ? result.current.error.message : "",
    ).toMatch(/Invalid workbench configuration/);
  });

  it("rejects a configuration whose phase holds a malformed group", async () => {
    const persona = { source: "builtin", text: "p" };
    server.use(
      http.get("/api/v1/workspaces/ws-nested/workbench", () =>
        HttpResponse.json({
          data: {
            revision: "r1",
            actions: { be: [null], se: [], ie: [{ id: "g", actions: [{}] }] },
            personas: { be: persona, se: persona, ie: persona },
            config: { status: "builtin" },
          },
        }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkbenchConfig("ws-nested", true), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("error"));
  });

  it.each([
    ["an action without a kind", { name: "g", actions: [{ id: "a" }] }],
    ["a group without a name", { actions: [{ kind: "chat" }] }],
    [
      "a menu whose items are not a list",
      { name: "g", actions: [{ kind: "menu", items: "x" }] },
    ],
    [
      "a menu item without a kind",
      { name: "g", actions: [{ kind: "menu", items: [{ id: "i" }] }] },
    ],
  ])("rejects a configuration holding %s", async (_case, group) => {
    const persona = { source: "builtin", text: "p" };
    server.use(
      http.get("/api/v1/workspaces/ws-case/workbench", () =>
        HttpResponse.json({
          data: {
            revision: "r1",
            actions: { be: [], se: [], ie: [group] },
            personas: { be: persona, se: persona, ie: persona },
            config: { status: "builtin" },
          },
        }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkbenchConfig("ws-case", true), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("error"));
  });

  it("fetches the served configuration only when enabled", async () => {
    const { wrapper } = createWrapper();
    const disabled = renderHook(() => useWorkbenchConfig("ws-1", false), {
      wrapper,
    });
    expect(disabled.result.current.status).toBe("loading");

    const { result } = renderHook(() => useWorkbenchConfig("ws-1", true), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status !== "ok") throw new Error("unreachable");
    expect(result.current.data.config.status).toBe("builtin");
    expect(result.current.data.actions.be[0].name).toBe("Author");
    expect(result.current.data.revision).toBe("mock-head.builtin0000");
  });
});

describe("useStartWorkbenchAction", () => {
  it("posts the workbench fields with a request key and seeds the session caches", async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/ws-1/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            {
              data: {
                session: startedSession,
                message: {
                  messageId: "in-1",
                  status: "queued",
                  createdAt: "2026-10-06T00:00:00Z",
                },
                assistantMessageId: "in-1",
                taskId: "task-1",
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useStartWorkbenchAction(), { wrapper });

    const response = await act(() =>
      result.current.mutateAsync({
        workspaceId: "ws-1",
        persona: "be",
        phase: "be",
        actionId: "build-mock",
        context: { kind: "phase" },
        configRevision: "rev-1",
        content: "",
        agentLaunch,
        scopeRef: "ws-1",
      }),
    );
    expect(response.session.id).toBe("session-wb");
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      content: "",
      scopeType: "workspace",
      scopeRef: "ws-1",
      persona: "be",
      phase: "be",
      actionId: "build-mock",
      workbenchContext: { kind: "phase" },
      configRevision: "rev-1",
    });
    expect(String(bodies[0].requestKey)).toMatch(/^workbench-/);
    expect(
      queryClient.getQueryData<unknown[]>(["chat-sessions", "ws-1", undefined]),
    ).toHaveLength(1);
    expect(queryClient.getQueryData(["chat-session", "session-wb"])).toEqual(
      startedSession,
    );
  });

  it("drops the cached configuration and names the retry on a stale revision", async () => {
    server.use(
      http.post("/api/v1/workspaces/ws-1/sessions/start", () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message:
                "The workbench configuration changed; reload it and try again",
              details: {
                code: "WORKBENCH_CONFIG_STALE",
                requestedRevision: "rev-0",
                currentRevision: "rev-1",
              },
            },
          },
          { status: 409 },
        ),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(workbenchConfigKey("ws-1"), { revision: "rev-0" });
    const { result } = renderHook(() => useStartWorkbenchAction(), { wrapper });

    let failure: unknown = null;
    await act(async () => {
      try {
        await result.current.mutateAsync({
          workspaceId: "ws-1",
          persona: "be",
          phase: "be",
          actionId: "build-mock",
          configRevision: "rev-0",
          content: "",
          agentLaunch,
        });
      } catch (error) {
        failure = error;
      }
    });
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).toBe(WORKBENCH_CONFIG_STALE_MESSAGE);
    // The cached configuration is invalidated so the ribbon refetches.
    await waitFor(() =>
      expect(
        queryClient.getQueryState(workbenchConfigKey("ws-1"))?.isInvalidated,
      ).toBe(true),
    );
  });

  it("passes other errors through unchanged", async () => {
    server.use(
      http.post("/api/v1/workspaces/ws-1/sessions/start", () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "unknown workbench action `nope`",
              details: {},
            },
          },
          { status: 422 },
        ),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useStartWorkbenchAction(), { wrapper });
    let failure: unknown = null;
    await act(async () => {
      try {
        await result.current.mutateAsync({
          workspaceId: "ws-1",
          persona: "be",
          phase: "be",
          actionId: "nope",
          configRevision: "rev-1",
          content: "",
          agentLaunch,
        });
      } catch (error) {
        failure = error;
      }
    });
    expect((failure as Error).message).toMatch(/unknown workbench action/);
  });
});

describe("useStartWorkbenchAction retries", () => {
  it("reuses the frozen request key and payload after a lost response, then rotates after success", async () => {
    const bodies: Record<string, unknown>[] = [];
    let attempts = 0;
    server.use(
      http.post(
        "/api/v1/workspaces/ws-1/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          attempts += 1;
          if (attempts === 1) return HttpResponse.error();
          return HttpResponse.json(
            {
              data: {
                session: startedSession,
                message: {
                  messageId: "in-1",
                  status: "queued",
                  createdAt: "2026-10-06T00:00:00Z",
                },
                assistantMessageId: "in-1",
                taskId: "task-1",
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useStartWorkbenchAction(), { wrapper });
    const input = {
      workspaceId: "ws-1",
      persona: "be" as const,
      phase: "be" as const,
      actionId: "build-mock",
      context: { kind: "phase" as const },
      configRevision: "rev-1",
      content: "",
      agentLaunch,
    };
    let failure: unknown = null;
    await act(async () => {
      try {
        await result.current.mutateAsync(input);
      } catch (error) {
        failure = error;
      }
    });
    expect(failure).toBeInstanceOf(Error);
    await act(async () => {
      await result.current.mutateAsync(input);
    });
    expect(bodies).toHaveLength(2);
    expect(bodies[1]).toEqual(bodies[0]);
    expect(String(bodies[0].requestKey)).toMatch(/^workbench-/);
    // A new submission after success gets its own key.
    await act(async () => {
      await result.current.mutateAsync(input);
    });
    expect(bodies[2].requestKey).not.toBe(bodies[0].requestKey);
  });
});

describe("useFollowView", () => {
  it("reuses the frozen request key and payload after a lost response", async () => {
    const bodies: Record<string, unknown>[] = [];
    let attempts = 0;
    server.use(
      http.post("/api/v1/sessions/session-a/fork", async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        attempts += 1;
        if (attempts === 1) return HttpResponse.error();
        return HttpResponse.json(
          { data: { sessionId: "session-b", messagesCopied: 1 } },
          { status: 201 },
        );
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useFollowView(), { wrapper });
    const input = {
      sessionId: "session-a",
      workspaceId: "ws-1",
      atIndex: 1,
      persona: "se" as const,
      configRevision: "rev-1",
    };
    await act(async () => {
      try {
        await result.current.mutateAsync(input);
      } catch {
        // lost response
      }
    });
    await act(async () => {
      await result.current.mutateAsync(input);
    });
    expect(bodies).toHaveLength(2);
    expect(bodies[1]).toEqual(bodies[0]);
    expect(String(bodies[0].requestKey)).toMatch(/^follow-view-/);
  });

  it("forks with the persona, the pinned revision and a request key", async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post("/api/v1/sessions/session-a/fork", async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(
          { data: { sessionId: "session-b", messagesCopied: 4 } },
          { status: 201 },
        );
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useFollowView(), { wrapper });
    const forked = await act(() =>
      result.current.mutateAsync({
        sessionId: "session-a",
        workspaceId: "ws-1",
        atIndex: 3,
        persona: "se",
        configRevision: "rev-1",
      }),
    );
    expect(forked).toEqual({ sessionId: "session-b", messagesCopied: 4 });
    expect(bodies[0]).toMatchObject({
      fromMessageIndex: 3,
      persona: "se",
      configRevision: "rev-1",
    });
    expect(String(bodies[0].requestKey)).toMatch(/^follow-view-/);
  });
});
