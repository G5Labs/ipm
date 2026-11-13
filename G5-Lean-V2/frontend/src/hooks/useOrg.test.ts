// =============================================================================
// useOrg.test.ts — Organization Hook Integration Tests with MSW
// =============================================================================

import { describe, it, expect } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";

import { server } from "../test/msw/server";
import type { CurrentUserDTO } from "../lib/types";
import { useOrgs } from "./useOrg";

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

const mockCurrentUser: CurrentUserDTO = {
  id: "user-alex",
  email: "alex@g5labs.dev",
  displayName: "Alex Chen",
  avatarUrl: null,
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
      orgId: "org-operator",
      orgName: "Platform operator",
      orgSlug: "operator",
      role: "viewer",
      orgKind: "operator",
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

// Self-serve organization creation is gone (g5.organization.creation); the
// caller's memberships, with each organization's kind, come from /users/me.
describe("useOrgs", () => {
  it("selects the memberships, including the operator organization's kind", async () => {
    server.use(
      http.get("/api/v1/users/me", () =>
        HttpResponse.json({ data: mockCurrentUser }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useOrgs(), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status !== "ok") return;
    expect(
      result.current.data.map(({ orgSlug, orgKind }) => [orgSlug, orgKind]),
    ).toEqual([
      ["g5labs", "tenant"],
      ["operator", "operator"],
    ]);
  });
});
