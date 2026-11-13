// =============================================================================
// useSettings.test.ts — Settings & Instance Hook Integration Tests with MSW
// =============================================================================

import { describe, it, expect, test } from "vitest";
import React from "react";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/msw/server";
import {
  useWorkspaceSettings,
  useUpdateSettings,
  useInstanceInfo,
  useWorkspaceProviderQuota,
} from "./useSettings";
import type {
  WorkspaceSettings,
  SpendBreakdownData,
  InstanceInfoDTO,
  HealthStatus,
  SubscriptionUsage,
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

const mockSettings: WorkspaceSettings = {
  workspaceId: "ws-123",
  maxSpendUsd: 100,
  spendPeriod: "daily",
  spendPeriodStart: "2024-11-15T00:00:00Z",
  maxInputTokens: null,
  maxOutputTokens: null,
  idleTimeoutSecs: 900,
  responseDeadlineSecs: 900,
  boundedExecutionSlots: 2,
  permissionPolicy: null,
  agentMayModifyOtherWorkspaces: false,
  updatedBy: "user-alex",
  updatedAt: "2024-11-15T10:00:00Z",
};

const mockSpend: SpendBreakdownData = {
  periodStart: "2024-11-15T00:00:00Z",
  periodEnd: "2024-11-16T00:00:00Z",
  totalSpendUsd: 45.5,
  totalInputTokens: 100000,
  totalOutputTokens: 50000,
  limitSpendUsd: 100,
  limitInputTokens: null,
  limitOutputTokens: null,
  isSpendLimitReached: false,
  unattributedSpendUsd: 0,
  otherTaskSpendUsd: 0,
  byTask: [
    {
      taskId: "task-1",
      taskName: "Compile",
      taskType: "compile",
      spendUsd: 25.0,
      inputTokens: 60000,
      outputTokens: 30000,
    },
    {
      taskId: "task-2",
      taskName: "Uplift",
      taskType: "uplift",
      spendUsd: 20.5,
      inputTokens: 40000,
      outputTokens: 20000,
    },
  ],
};

const mockInstanceInfo: InstanceInfoDTO = {
  version: "0.1.0",
  features: ["chat"],
  authProviders: ["github"],
  maxUploadSizeMb: 50,
  selfHosted: false,
  devLoginEnabled: false,
  motd: "Welcome to G5!",
};

const mockHealth: HealthStatus = {
  status: "healthy",
  checks: {
    database: "healthy",
  },
  uptime: 86400,
};

const mockModels: string[] = [
  "claude-sonnet-4-20250514",
  "claude-3-5-haiku-20241022",
  "gpt-4o",
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("useWorkspaceSettings", () => {
  it("fetches workspace settings", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id/settings", () =>
        HttpResponse.json({ data: mockSettings }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkspaceSettings("ws-123"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.workspaceId).toBe("ws-123");
      expect(result.current.data.idleTimeoutSecs).toBe(900);
      expect(result.current.data.maxSpendUsd).toBe(100);
    }
  });
});

describe("useUpdateSettings", () => {
  it("updates settings and invalidates cache", async () => {
    const updatedSettings: WorkspaceSettings = {
      ...mockSettings,
      idleTimeoutSecs: 1800,
    };

    server.use(
      http.put("/api/v1/workspaces/:id/settings", () =>
        HttpResponse.json({ data: updatedSettings }),
      ),
    );

    const { wrapper, queryClient } = createWrapper();

    // Pre-populate cache
    queryClient.setQueryData(["settings", "ws-123"], mockSettings);

    const { result } = renderHook(() => useUpdateSettings("ws-123"), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({ idleTimeoutSecs: 1800 });
    });

    // Verify cache was invalidated
    await waitFor(() => {
      const state = queryClient.getQueryState(["settings", "ws-123"]);
      expect(state?.isInvalidated).toBe(true);
    });
  });
});

describe("useInstanceInfo", () => {
  it("fetches instance info", async () => {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({ data: mockInstanceInfo }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useInstanceInfo(), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.version).toBe("0.1.0");
      expect(result.current.data.features).toEqual(["chat"]);
    }
  });
});

// ---------------------------------------------------------------------------
// useWorkspaceProviderQuota — account-keyed pre-session snapshot (hooks D22;
// settings-page D18; agent_interaction_ux D8)
// ---------------------------------------------------------------------------

describe("useWorkspaceProviderQuota", () => {
  const snapshot: SubscriptionUsage = {
    fiveHourUtilization: 0.4,
    fiveHourResetAt: "2026-08-19T15:00:00Z",
    sevenDayUtilization: 0.7,
    sevenDayResetAt: "2026-08-24T12:00:00Z",
    capturedAt: "2026-08-19T11:59:00Z",
  };

  function recordQuotaRequests() {
    const searches: string[] = [];
    server.use(
      http.get("/api/v1/workspaces/:id/provider-quota", ({ request }) => {
        searches.push(new URL(request.url).search);
        return HttpResponse.json({ data: snapshot });
      }),
    );
    return searches;
  }

  it("appends accountId to the URL and keys the query by it", async () => {
    const searches = recordQuotaRequests();
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => useWorkspaceProviderQuota("ws-123", true, "acc-1"),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(searches).toEqual([
      "?provider=claude_code&authMode=anthropic_oauth&accountId=acc-1",
    ]);
    expect(
      queryClient.getQueryData(["provider-quota", "ws-123", "acc-1"]),
    ).toEqual(snapshot);
    expect(
      queryClient.getQueryData(["provider-quota", "ws-123", null]),
    ).toBeUndefined();
  });

  it("omits accountId from the URL and keys the query by null without one", async () => {
    const searches = recordQuotaRequests();
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => useWorkspaceProviderQuota("ws-123", true),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(searches).toEqual([
      "?provider=claude_code&authMode=anthropic_oauth",
    ]);
    expect(
      queryClient.getQueryData(["provider-quota", "ws-123", null]),
    ).toEqual(snapshot);
  });

  it("refetches under the new key when the account changes", async () => {
    const searches = recordQuotaRequests();
    const { wrapper } = createWrapper();
    const { result, rerender } = renderHook(
      ({ accountId }: { accountId?: string }) =>
        useWorkspaceProviderQuota("ws-123", true, accountId),
      {
        wrapper,
        initialProps: { accountId: undefined as string | undefined },
      },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));

    rerender({ accountId: "acc-2" });
    await waitFor(() =>
      expect(searches).toContain(
        "?provider=claude_code&authMode=anthropic_oauth&accountId=acc-2",
      ),
    );
  });

  it("does not fetch while disabled", async () => {
    const searches = recordQuotaRequests();
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useWorkspaceProviderQuota("ws-123", false, "acc-1"),
      { wrapper },
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(result.current.status).toBe("loading");
    expect(searches).toEqual([]);
  });
});
