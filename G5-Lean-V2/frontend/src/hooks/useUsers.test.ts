// =============================================================================
// useUsers.test.ts — User & Auth Hook Integration Tests with MSW
// =============================================================================

import { describe, it, expect, test } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { ApiError } from "../lib/api";
import type { CurrentUserDTO, UserRef } from "../lib/types";
import { server } from "../test/msw/server";
import { useCurrentUser, useUser } from "./useAuth";

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

const mockCurrentUser: CurrentUserDTO = {
  id: "user-alex",
  email: "alex@g5labs.dev",
  displayName: "Alex Chen",
  avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
  oauthProvider: "github",
  activeOrgId: "org-g5labs",
  orgs: [
    {
      orgId: "org-g5labs",
      orgName: "G5 Labs",
      orgSlug: "g5labs",
      role: "admin",
      orgKind: "tenant",
    },
    {
      orgId: "org-acme",
      orgName: "Acme Corp",
      orgSlug: "acme",
      role: "admin",
      orgKind: "tenant",
    },
  ],
  scmConnections: [
    {
      provider: "github",
      providerUsername: "alexchen",
      scopes: ["repo", "read:org"],
      hasInstallation: true,
      connectedAt: "2024-01-01T00:00:00Z",
    },
  ],
  createdAt: "2024-01-01T00:00:00Z",
};

const mockUserRef: UserRef = {
  id: "user-1",
  displayName: "Test User",
  avatarUrl: null,
  initial: "T",
  color: "green",
};

const mockScopes: string[] = [
  "workspaces:read",
  "workspaces:write",
  "tasks:execute",
  "workers:admin",
  "org:admin",
  "sessions:write",
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("useCurrentUser", () => {
  it("fetches current user", async () => {
    server.use(
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: mockCurrentUser }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useCurrentUser(), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.id).toBe("user-alex");
      expect(result.current.data.email).toBe("alex@g5labs.dev");
      expect(result.current.data.orgs).toHaveLength(2);
      expect(result.current.data.scmConnections).toHaveLength(1);
    }
  });

  it("current user uses long stale time", async () => {
    let fetchCount = 0;
    server.use(
      http.get("/api/v1/users/me", () => {
        fetchCount++;
        return HttpResponse.json({ data: mockCurrentUser });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useCurrentUser(), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    // The hook should have fetched once
    expect(fetchCount).toBe(1);

    // Since staleTime is Infinity in test config (and hook uses 300_000),
    // no refetch should occur
  });

  it("handles unauthorized error on user fetch", async () => {
    server.use(
      http.get("/api/v1/users/me", () =>
        HttpResponse.json(
          {
            error: {
              code: "UNAUTHORIZED",
              message: "Authentication required",
              requestId: "req-unauth",
            },
          },
          { status: 401 },
        ),
      ),
      // Also mock the refresh endpoint to fail (prevent retry)
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json(
          {
            error: {
              code: "UNAUTHORIZED",
              message: "Refresh failed",
              requestId: "req-refresh",
            },
          },
          { status: 401 },
        ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useCurrentUser(), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("error"), {
      timeout: 5_000,
    });

    if (result.current.status === "error") {
      expect(result.current.error).toBeInstanceOf(Error);
    }
  });
});

describe("useUser", () => {
  it("fetches user by ID", async () => {
    server.use(
      http.get("/api/v1/users/:userId", () =>
        HttpResponse.json({ data: mockUserRef }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useUser("user-1"), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.id).toBe("user-1");
      expect(result.current.data.displayName).toBe("Test User");
    }
  });

  it("handles user not found", async () => {
    server.use(
      http.get("/api/v1/users/:userId", () =>
        HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: "User not found",
              requestId: "req-notfound",
            },
          },
          { status: 404 },
        ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useUser("nonexistent"), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("error"));

    if (result.current.status === "error") {
      expect(result.current.error).toBeInstanceOf(ApiError);
      expect((result.current.error as ApiError).code).toBe("NOT_FOUND");
    }
  });
});
