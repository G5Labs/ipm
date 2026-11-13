// =============================================================================
// useProviderAccounts.test.ts — list/create/update/delete hooks (settings-page
// D16). MSW mocks the HTTP layer; the hooks and the React Query wiring are real.
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import type { ProviderAccountDTO } from "../lib/types";
import {
  providerAccountsQueryKey,
  useCreateProviderAccount,
  useDeleteProviderAccount,
  useProviderAccounts,
  useUpdateProviderAccount,
} from "./useProviderAccounts";

const ORG_ID = "org-g5labs";
const LIST_URL = `/api/v1/orgs/${ORG_ID}/provider-accounts`;

const ROW: ProviderAccountDTO = {
  id: "acc-1",
  orgId: ORG_ID,
  name: "Personal Max",
  kind: "claude_subscription",
  family: "claude",
  scope: "user",
  workspaceId: null,
  userId: "user-alex",
  baseUrl: null,
  hasSecret: true,
  secretHint: "sk-ant-oat01",
  createdBy: "user-alex",
  createdAt: "2026-07-02T10:00:00Z",
  updatedAt: "2026-07-02T10:00:00Z",
  usage: null,
};

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  return { wrapper, queryClient };
}

describe("useProviderAccounts", () => {
  it("keys the list under the scoped-config eviction prefix", () => {
    expect(providerAccountsQueryKey(ORG_ID, "ws-1")).toEqual([
      "scoped-config",
      "provider-accounts",
      ORG_ID,
      "ws-1",
    ]);
    expect(providerAccountsQueryKey(ORG_ID)).toEqual([
      "scoped-config",
      "provider-accounts",
      ORG_ID,
      null,
    ]);
  });

  it("lists accounts for the workspace in context and caches them under that key", async () => {
    const seen: Array<string | null> = [];
    server.use(
      http.get(LIST_URL, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get("workspaceId"));
        return HttpResponse.json({ data: [ROW] });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(() => useProviderAccounts(ORG_ID, "ws-1"), {
      wrapper,
    });

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(result.current.status === "ok" && result.current.data).toEqual([
      ROW,
    ]);
    expect(seen).toEqual(["ws-1"]);
    expect(
      queryClient.getQueryData(providerAccountsQueryKey(ORG_ID, "ws-1")),
    ).toEqual([ROW]);
  });

  it("omits the workspaceId param without a workspace and stays disabled without an org", async () => {
    const seen: Array<string | null> = [];
    server.use(
      http.get(LIST_URL, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get("workspaceId"));
        return HttpResponse.json({ data: [] });
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProviderAccounts(ORG_ID), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(seen).toEqual([null]);

    const disabled = renderHook(() => useProviderAccounts(""), { wrapper });
    expect(disabled.result.current.status).toBe("loading");
  });

  it("create/update/delete hit the wire contract, discard bodies, and refetch the list", async () => {
    const calls: Array<{ method: string; path: string; body?: unknown }> = [];
    let listFetches = 0;
    server.use(
      http.get(LIST_URL, () => {
        listFetches += 1;
        return HttpResponse.json({ data: [ROW] });
      }),
      http.post(LIST_URL, async ({ request }) => {
        calls.push({ method: "POST", path: "/", body: await request.json() });
        return HttpResponse.json({ data: ROW }, { status: 201 });
      }),
      http.patch(`${LIST_URL}/:id`, async ({ request, params }) => {
        calls.push({
          method: "PATCH",
          path: `/${String(params.id)}`,
          body: await request.json(),
        });
        return HttpResponse.json({ data: ROW });
      }),
      http.delete(`${LIST_URL}/:id`, ({ params }) => {
        calls.push({ method: "DELETE", path: `/${String(params.id)}` });
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        list: useProviderAccounts(ORG_ID, "ws-1"),
        create: useCreateProviderAccount(ORG_ID),
        update: useUpdateProviderAccount(ORG_ID),
        remove: useDeleteProviderAccount(ORG_ID),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.list.status).toBe("ok"));
    expect(listFetches).toBe(1);

    let created: unknown = "unset";
    await act(async () => {
      created = await result.current.create.mutateAsync({
        name: "New",
        kind: "anthropic_api_key",
        scope: "user",
        secret: "sk-ant-api03-0123456789abcdef",
      });
    });
    expect(created).toBeUndefined();
    await waitFor(() => expect(listFetches).toBe(2));

    await act(async () => {
      await result.current.update.mutateAsync({
        accountId: "acc-1",
        patch: { name: "Renamed" },
      });
    });
    await waitFor(() => expect(listFetches).toBe(3));

    await act(async () => {
      await result.current.remove.mutateAsync("acc-1");
    });
    await waitFor(() => expect(listFetches).toBe(4));

    expect(calls).toEqual([
      {
        method: "POST",
        path: "/",
        body: {
          name: "New",
          kind: "anthropic_api_key",
          scope: "user",
          secret: "sk-ant-api03-0123456789abcdef",
        },
      },
      { method: "PATCH", path: "/acc-1", body: { name: "Renamed" } },
      { method: "DELETE", path: "/acc-1" },
    ]);
    // Discarded bodies: no hint-bearing DTO lives in mutation state.
    expect(result.current.create.data).toBeUndefined();
    expect(result.current.update.data).toBeUndefined();
  });
});
