// =============================================================================
// useWorkerHosts.test.ts — Worker-Host (org-scoped cloud VM) Hook Tests with MSW
// =============================================================================
// Exercises the data layer for the org-scoped `worker_hosts` control plane:
// list (with the provisioning→ready polling transition), provision, and
// deallocate. Backed by the stateful MSW worker-host store in
// test/msw/handlers.ts (reset per test for isolation).
// =============================================================================

import { describe, it, expect, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor, act } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { ApiError } from "../lib/api";
import type { ListWorkerHostsResponse, WorkerHost } from "../lib/types";
import { server } from "../test/msw/server";
import {
  resetMockWorkerHosts,
  seedMockLegacyWorkerHostWithoutImage,
} from "../test/msw/handlers";
import {
  useWorkerHosts,
  useProvisionWorkerHost,
  useDeallocateWorkerHost,
  useRefreshWorkerHostRuntimeLabels,
  workerHostsRefetchInterval,
} from "./useWorkerHosts";

// ---------------------------------------------------------------------------
// Test Wrapper
// ---------------------------------------------------------------------------

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  });

  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);

  return { wrapper, queryClient };
}

const ORG_ID = "org-test";
const WORKER_HOSTS_KEY = ["org-worker-hosts", ORG_ID];

function hostWith(status: WorkerHost["status"]): WorkerHost {
  return {
    hostId: "h",
    orgId: ORG_ID,
    instanceType: "n2-standard-8",
    region: "us-central1",
    status,
    createdAt: "2026-05-01T00:00:00Z",
  };
}

function hostList(
  ...statuses: WorkerHost["status"][]
): ListWorkerHostsResponse {
  return {
    hosts: statuses.map(hostWith),
    total: statuses.length,
    promotedImageTracks: [],
  };
}

beforeEach(() => {
  resetMockWorkerHosts();
});

// ---------------------------------------------------------------------------
// useWorkerHosts — list
// ---------------------------------------------------------------------------

describe("useWorkerHosts", () => {
  it("requests active hosts so terminated history cannot crowd out the page", async () => {
    let activeOnly: string | null = null;
    server.use(
      http.get("/api/v1/orgs/:orgId/worker-hosts", ({ request }) => {
        activeOnly = new URL(request.url).searchParams.get("activeOnly");
        return HttpResponse.json({
          data: { hosts: [], total: 0, promotedImageTracks: [] },
        });
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkerHosts(ORG_ID), { wrapper });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(activeOnly).toBe("true");
  });

  it("returns the org's worker hosts", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/worker-hosts", () =>
        HttpResponse.json({
          data: {
            hosts: [
              {
                hostId: "host-a",
                orgId: ORG_ID,
                instanceType: "n2-standard-8",
                region: "us-central1",
                status: "ready",
                createdAt: "2026-05-01T00:00:00Z",
                lastHeartbeat: "2026-05-01T00:01:00Z",
              },
              {
                hostId: "host-b",
                orgId: ORG_ID,
                instanceType: "n2-standard-16",
                region: "europe-west1",
                status: "provisioning",
                createdAt: "2026-05-02T00:00:00Z",
              },
            ],
            total: 2,
            promotedImageTracks: [],
          } satisfies ListWorkerHostsResponse,
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkerHosts(ORG_ID), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data.total).toBe(2);
      expect(result.current.data.hosts).toHaveLength(2);
      const [a, b] = result.current.data.hosts;
      expect(a.hostId).toBe("host-a");
      expect(a.status).toBe("ready");
      expect(a.lastHeartbeat).toBe("2026-05-01T00:01:00Z");
      // Absent optionals stay absent (not null) on the parsed DTO.
      expect(b.lastHeartbeat).toBeUndefined();
      expect(b.region).toBe("europe-west1");
    }
  });

  it("does not fetch when orgId is empty", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkerHosts(""), { wrapper });
    // Disabled query stays in the loading state with no network call (an
    // unmocked call would fail the test via onUnhandledRequest: 'error').
    expect(result.current.status).toBe("loading");
  });

  it("surfaces a 403 as a graceful error AsyncState", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/worker-hosts", () =>
        HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "missing workers:admin",
              requestId: "req-wh-403",
            },
          },
          { status: 403 },
        ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useWorkerHosts(ORG_ID), { wrapper });

    await waitFor(() => expect(result.current.status).toBe("error"));

    if (result.current.status === "error") {
      expect(result.current.error).toBeInstanceOf(ApiError);
      expect((result.current.error as ApiError).status).toBe(403);
      expect((result.current.error as ApiError).code).toBe("FORBIDDEN");
    }
  });
});

// ---------------------------------------------------------------------------
// useProvisionWorkerHost
// ---------------------------------------------------------------------------

describe("useProvisionWorkerHost", () => {
  it("POSTs region/instanceType, applies the default, and invalidates the list", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        list: useWorkerHosts(ORG_ID),
        provision: useProvisionWorkerHost(ORG_ID),
      }),
      { wrapper },
    );

    // Initial list is empty.
    await waitFor(() => expect(result.current.list.status).toBe("ok"));
    if (result.current.list.status === "ok") {
      expect(result.current.list.data.hosts).toHaveLength(0);
    }

    // Provision without an explicit instanceType — the server applies the
    // c3-standard-4 default and echoes the region back (proving the body).
    let provisioned: Awaited<
      ReturnType<typeof result.current.provision.mutateAsync>
    >;
    await act(async () => {
      provisioned = await result.current.provision.mutateAsync({
        region: "us-east1",
        imageTrack: "current",
      });
    });

    expect(provisioned!.region).toBe("us-east1");
    expect(provisioned!.instanceType).toBe("c3-standard-4");
    expect(provisioned!.status).toBe("provisioning");
    expect(provisioned!.createdAt).toBeDefined();
    expect(provisioned!.baseImageSourceKind).toBe("track");
    expect(provisioned!.baseImageSourceValue).toBe("current");
    // The one-time host credential is stripped at RUNTIME (not merely hidden by
    // the type): it never surfaces to consumers or the mutation cache. The mock
    // POST returns a fake `hostJwt`, so these assertions prove the strip works.
    expect(provisioned!).not.toHaveProperty("hostJwt");
    expect(JSON.stringify(provisioned!)).not.toContain("hostJwt");

    // onSuccess invalidation refetches the list, which now shows the new
    // provisioning row.
    await waitFor(() => {
      if (result.current.list.status !== "ok") throw new Error("not ok");
      expect(result.current.list.data.hosts).toHaveLength(1);
    });

    if (result.current.list.status === "ok") {
      const host = result.current.list.data.hosts[0];
      expect(host.region).toBe("us-east1");
      expect(host.instanceType).toBe("c3-standard-4");
      expect(host.status).toBe("provisioning");
    }
  });
});

// ---------------------------------------------------------------------------
// useProvisionWorkerHost — runtime/profile echo + body shape (PR3)
// ---------------------------------------------------------------------------

const DIGEST_REF = `reg.example.com/img@sha256:${"a".repeat(64)}`;

describe("useProvisionWorkerHost runtime/profile (PR3)", () => {
  it("echoes runtime/profile fields and still strips hostJwt", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProvisionWorkerHost(ORG_ID), {
      wrapper,
    });

    let provisioned: Awaited<ReturnType<typeof result.current.mutateAsync>>;
    await act(async () => {
      provisioned = await result.current.mutateAsync({
        region: "us-east1",
        baseImageRef: DIGEST_REF,
        workerProfile: "slim",
      });
    });

    // The echoed runtime/profile fields survive the reconstruction…
    expect(provisioned!.workerProfile).toBe("slim");
    expect(provisioned!.baseImageRef).toBe(DIGEST_REF);
    expect(provisioned!.baseImageDigest).toBe(`sha256:${"a".repeat(64)}`);
    expect(provisioned!.runtimeLabels).toEqual({});
    // …while the one-time host credential is still stripped at RUNTIME.
    expect(provisioned!).not.toHaveProperty("hostJwt");
    expect(JSON.stringify(provisioned!)).not.toContain("hostJwt");
  });

  it("always sends workerProfile and omits a blank baseImageRef from the body", async () => {
    let postBody: Record<string, unknown> | null = null;
    server.use(
      http.post("/api/v1/orgs/:orgId/worker-hosts", async ({ request }) => {
        postBody = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          {
            data: {
              hostId: "host-x",
              hostJwt: "mock-host-jwt",
              status: "provisioning",
              region: (postBody as { region: string }).region,
              instanceType: "n2-standard-8",
              createdAt: "2026-05-30T00:00:00Z",
              workerProfile: "fat-default",
              runtimeLabels: {},
            },
          },
          { status: 201 },
        );
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProvisionWorkerHost(ORG_ID), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        region: "us-east1",
        workerProfile: "fat-default",
      });
    });

    expect(postBody).not.toBeNull();
    expect(postBody!).toMatchObject({
      region: "us-east1",
      workerProfile: "fat-default",
    });
    // A blank/omitted baseImageRef is never sent.
    expect(postBody!).not.toHaveProperty("baseImageRef");
  });

  it("passes workspaceVmLimit through to the POST body when provided", async () => {
    let postBody: Record<string, unknown> | null = null;
    server.use(
      http.post("/api/v1/orgs/:orgId/worker-hosts", async ({ request }) => {
        postBody = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          {
            data: {
              hostId: "host-cj",
              hostJwt: "mock-host-jwt",
              status: "provisioning",
              region: (postBody as { region: string }).region,
              instanceType: "n2-standard-8",
              createdAt: "2026-05-30T00:00:00Z",
              workerProfile: "fat-default",
              runtimeLabels: {},
            },
          },
          { status: 201 },
        );
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProvisionWorkerHost(ORG_ID), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        region: "us-east1",
        instanceType: "n2-standard-8",
        workspaceVmLimit: 2,
      });
    });

    expect(postBody!).toMatchObject({ workspaceVmLimit: 2 });
  });

  it("rejects tag-only baseImageRef like the backend provision endpoint", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProvisionWorkerHost(ORG_ID), {
      wrapper,
    });

    await expect(
      result.current.mutateAsync({
        region: "us-east1",
        baseImageRef: "reg.example.com/img:latest",
        workerProfile: "fat-default",
      }),
    ).rejects.toMatchObject({
      status: 422,
      code: "VALIDATION_ERROR",
      details: expect.objectContaining({ field: "baseImageRef" }),
    });
  });

  it("rejects unsupported workerProfile values like the backend provision endpoint", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useProvisionWorkerHost(ORG_ID), {
      wrapper,
    });

    await expect(
      result.current.mutateAsync({
        region: "us-east1",
        imageTrack: "current",
        workerProfile: "turbo" as never,
      }),
    ).rejects.toMatchObject({
      status: 422,
      code: "VALIDATION_ERROR",
      details: expect.objectContaining({ field: "workerProfile" }),
    });
  });
});

// ---------------------------------------------------------------------------
// useRefreshWorkerHostRuntimeLabels (PR3)
// ---------------------------------------------------------------------------

type CombinedRefreshHooks = {
  list: ReturnType<typeof useWorkerHosts>;
  provision: ReturnType<typeof useProvisionWorkerHost>;
  refresh: ReturnType<typeof useRefreshWorkerHostRuntimeLabels>;
};

/** Provision one host (optionally with a baseImageRef) and return its id. */
async function provisionHostId(
  result: { current: Pick<CombinedRefreshHooks, "provision"> },
  baseImageRef?: string,
): Promise<string> {
  let hostId = "";
  await act(async () => {
    const r = await result.current.provision.mutateAsync({
      region: "us-central1",
      ...(baseImageRef ? { baseImageRef } : { imageTrack: "current" }),
    });
    hostId = r.hostId;
  });
  return hostId;
}

describe("useRefreshWorkerHostRuntimeLabels", () => {
  it("populates labels on success and invalidates the list", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        list: useWorkerHosts(ORG_ID),
        provision: useProvisionWorkerHost(ORG_ID),
        refresh: useRefreshWorkerHostRuntimeLabels(ORG_ID),
      }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.list.status).toBe("ok"));
    const hostId = await provisionHostId(result, DIGEST_REF);

    let refreshed: WorkerHost | undefined;
    await act(async () => {
      refreshed = await result.current.refresh.mutateAsync({ hostId });
    });

    expect(refreshed!.runtimeSchema).toBe("v1");
    expect(refreshed!.runtimeLabels).toMatchObject({
      "com.g5labs.runtime.base_image": "debian-bookworm",
    });
    expect(refreshed!.runtimeLabels).not.toHaveProperty(
      "com.g5labs.runtime.schema",
    );
    // Resolved labels live in the G5-owned namespace and never the reserved
    // OCI `org.opencontainers.image.*` namespace (locked decision).
    for (const key of Object.keys(refreshed!.runtimeLabels ?? {})) {
      expect(key.startsWith("org.opencontainers.image.")).toBe(false);
    }
    expect(refreshed!.runtimeLabelsRefreshedAt).toBeDefined();

    // The onSuccess invalidation refetched the list with the refreshed row.
    await waitFor(() => {
      if (result.current.list.status !== "ok") throw new Error("not ok");
      const host = result.current.list.data.hosts.find(
        (h) => h.hostId === hostId,
      );
      expect(host?.runtimeSchema).toBe("v1");
    });
  });

  it("resolves the absent-schema success state (schema null, labels {})", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        list: useWorkerHosts(ORG_ID),
        provision: useProvisionWorkerHost(ORG_ID),
        refresh: useRefreshWorkerHostRuntimeLabels(ORG_ID),
      }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.list.status).toBe("ok"));
    const hostId = await provisionHostId(
      result,
      `reg.example.com/no-schema@sha256:${"d".repeat(64)}`,
    );

    let refreshed: WorkerHost | undefined;
    await act(async () => {
      refreshed = await result.current.refresh.mutateAsync({ hostId });
    });

    // Absent-schema is a SUCCESS, not an error: schema absent, labels empty.
    expect(refreshed!.runtimeSchema).toBeUndefined();
    expect(refreshed!.runtimeLabels).toEqual({});
    expect(refreshed!.runtimeLabelsRefreshedAt).toBeDefined();
  });

  it("rejects with 422 no_base_image_ref when the host has no base image ref", async () => {
    const hostId = seedMockLegacyWorkerHostWithoutImage(ORG_ID);
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useRefreshWorkerHostRuntimeLabels(ORG_ID),
      { wrapper },
    );

    await expect(result.current.mutateAsync({ hostId })).rejects.toMatchObject({
      status: 422,
      code: "VALIDATION_ERROR",
      details: expect.objectContaining({ code: "no_base_image_ref" }),
    });
  });

  it("rejects with 422 unsupported_runtime_schema for an unknown schema", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        provision: useProvisionWorkerHost(ORG_ID),
        refresh: useRefreshWorkerHostRuntimeLabels(ORG_ID),
      }),
      { wrapper },
    );

    const hostId = await provisionHostId(
      result,
      `reg.example.com/unknown-schema@sha256:${"b".repeat(64)}`,
    );

    await expect(
      result.current.refresh.mutateAsync({ hostId }),
    ).rejects.toMatchObject({
      status: 422,
      code: "VALIDATION_ERROR",
      details: expect.objectContaining({ code: "unsupported_runtime_schema" }),
    });
  });

  it("rejects with 503 when the registry is unreachable", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        provision: useProvisionWorkerHost(ORG_ID),
        refresh: useRefreshWorkerHostRuntimeLabels(ORG_ID),
      }),
      { wrapper },
    );

    const hostId = await provisionHostId(
      result,
      `reg.example.com/registry-down@sha256:${"c".repeat(64)}`,
    );

    await expect(
      result.current.refresh.mutateAsync({ hostId }),
    ).rejects.toMatchObject({ status: 503 });
  });

  it("surfaces a 403 as a graceful rejection", async () => {
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/worker-hosts/:hostId/runtime-labels/refresh",
        () =>
          HttpResponse.json(
            {
              error: {
                code: "FORBIDDEN",
                message: "missing workers:admin",
                requestId: "req-refresh-403",
              },
            },
            { status: 403 },
          ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useRefreshWorkerHostRuntimeLabels(ORG_ID),
      { wrapper },
    );

    await expect(
      result.current.mutateAsync({ hostId: "host-1" }),
    ).rejects.toMatchObject({ status: 403, code: "FORBIDDEN" });
  });

  it("POSTs the refresh endpoint with no request body (PR2 contract)", async () => {
    let capturedBody = "unset";
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/worker-hosts/:hostId/runtime-labels/refresh",
        async ({ request, params }) => {
          capturedBody = await request.text();
          return HttpResponse.json({
            data: {
              hostId: params.hostId as string,
              orgId: params.orgId as string,
              region: "us-central1",
              instanceType: "n2-standard-8",
              status: "ready",
              createdAt: new Date().toISOString(),
              workerProfile: "fat-default",
              runtimeLabels: {},
            },
          });
        },
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useRefreshWorkerHostRuntimeLabels(ORG_ID),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync({ hostId: "host-1" });
    });

    // The PR2 refresh endpoint takes no request body; the hook must not send one.
    expect(capturedBody).toBe("");
  });
});

// ---------------------------------------------------------------------------
// Provisioning → ready transition (observed via polling)
// ---------------------------------------------------------------------------

describe("worker-host provisioning lifecycle", () => {
  it("observes the provisioning→ready transition across polls", async () => {
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => ({
        list: useWorkerHosts(ORG_ID),
        provision: useProvisionWorkerHost(ORG_ID),
      }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.list.status).toBe("ok"));

    await act(async () => {
      await result.current.provision.mutateAsync({
        region: "us-central1",
        instanceType: "n2-standard-16",
        imageTrack: "current",
      });
    });

    // First poll after provisioning: still provisioning.
    await waitFor(() => {
      if (result.current.list.status !== "ok") throw new Error("not ok");
      expect(result.current.list.data.hosts[0]?.status).toBe("provisioning");
    });

    // Drive the next poll (the same refetch the conditional refetchInterval
    // issues) — the mock reconciler flips the host to ready.
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: WORKER_HOSTS_KEY });
    });

    await waitFor(() => {
      if (result.current.list.status !== "ok") throw new Error("not ok");
      expect(result.current.list.data.hosts[0]?.status).toBe("ready");
    });

    if (result.current.list.status === "ok") {
      const host = result.current.list.data.hosts[0];
      expect(host.instanceType).toBe("n2-standard-16");
      expect(host.lastHeartbeat).toBeDefined();
    }
  });
});

// ---------------------------------------------------------------------------
// useDeallocateWorkerHost
// ---------------------------------------------------------------------------

describe("useDeallocateWorkerHost", () => {
  it("terminates a host and invalidates the list", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        list: useWorkerHosts(ORG_ID),
        provision: useProvisionWorkerHost(ORG_ID),
        deallocate: useDeallocateWorkerHost(ORG_ID),
      }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.list.status).toBe("ok"));

    let hostId = "";
    await act(async () => {
      const provisioned = await result.current.provision.mutateAsync({
        region: "us-west1",
        imageTrack: "current",
      });
      hostId = provisioned.hostId;
    });

    await waitFor(() => {
      if (result.current.list.status !== "ok") throw new Error("not ok");
      expect(result.current.list.data.hosts).toHaveLength(1);
    });

    await act(async () => {
      await result.current.deallocate.mutateAsync({ hostId });
    });

    // The row is transitioned to `terminated` (never deleted) and the list
    // refetch reflects it.
    await waitFor(() => {
      if (result.current.list.status !== "ok") throw new Error("not ok");
      expect(result.current.list.data.hosts).toEqual([]);
      expect(result.current.list.data.total).toBe(0);
    });
  });

  it("surfaces a 404 for an unknown host as a graceful error", async () => {
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useDeallocateWorkerHost(ORG_ID), {
      wrapper,
    });

    await expect(
      result.current.mutateAsync({ hostId: "does-not-exist" }),
    ).rejects.toMatchObject({ status: 404, code: "NOT_FOUND" });
  });
});

// ---------------------------------------------------------------------------
// workerHostsRefetchInterval — conditional polling decision (pure, deterministic)
// ---------------------------------------------------------------------------

describe("workerHostsRefetchInterval", () => {
  it("does not poll when data is undefined or the list is empty", () => {
    expect(workerHostsRefetchInterval(undefined)).toBe(false);
    expect(workerHostsRefetchInterval(hostList())).toBe(false);
  });

  it("polls every 6s while any host is provisioning or draining", () => {
    expect(workerHostsRefetchInterval(hostList("provisioning"))).toBe(6_000);
    expect(workerHostsRefetchInterval(hostList("draining"))).toBe(6_000);
    expect(workerHostsRefetchInterval(hostList("ready", "provisioning"))).toBe(
      6_000,
    );
  });

  it("stops polling once every host is stable (ready/terminated)", () => {
    expect(workerHostsRefetchInterval(hostList("ready"))).toBe(false);
    expect(workerHostsRefetchInterval(hostList("terminated"))).toBe(false);
    expect(workerHostsRefetchInterval(hostList("ready", "terminated"))).toBe(
      false,
    );
  });
});
