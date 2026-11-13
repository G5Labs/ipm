// =============================================================================
// useIntentLog hook tests (React Query + MSW).
// Test view: intent_log.test_view.md TC-ILAPI-001/002/003.
// =============================================================================

import { describe, it, expect } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { server } from "../test/msw/server";
import { useIntentLog, type IntentLogResponse } from "./useIntentLog";

function makeWrapper() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
  };
}

const SAMPLE: IntentLogResponse = {
  rows: [
    {
      id: "id-1",
      occurredAt: "2026-06-01T12:00:00Z",
      occurredAtSource: "record",
      actor: "Ziniu",
      actorSource: "record",
      sourceKind: "intent_record",
      intentKind: "ontology",
      authorType: "human",
      artifactType: "ontology",
      artifactPath: ".semlift/specs/x.semlift.md",
      blockId: "I1",
      contentPreview: "Initialize the feature module.",
      content: "Initialize the feature module.",
      isHuman: true,
      traceKey: {
        fileType: "ontology",
        fileName: ".semlift/specs/x.semlift.md",
        id: "I1",
      },
      traceStatus: "traceable",
      rawSource: { type: "ontology", id: "I1" },
    },
  ],
  meta: { facets: {}, cursor: null, hasMore: false, diagnostics: [] },
};

describe("useIntentLog", () => {
  it("returns rows + meta on success (TC-ILAPI-001)", async () => {
    server.use(
      http.get("*/api/v1/intent-log", () =>
        HttpResponse.json({ data: SAMPLE }),
      ),
    );
    const { result } = renderHook(() => useIntentLog("branch-1", {}), {
      wrapper: makeWrapper(),
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data.rows).toHaveLength(1);
      expect(result.current.data.rows[0].intentKind).toBe("ontology");
      expect(result.current.data.meta.diagnostics).toHaveLength(0);
    }
  });

  it("serializes array filters as comma-separated params (TC-ILAPI-002)", async () => {
    let capturedUrl = "";
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        capturedUrl = request.url;
        return HttpResponse.json({ data: SAMPLE });
      }),
    );
    const { result } = renderHook(
      () =>
        useIntentLog("branch-1", {
          intentKind: ["ontology", "view"],
          authorType: ["human", "system", "unknown"],
          traceStatus: ["traceable"],
        }),
      { wrapper: makeWrapper() },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));
    const params = new URL(capturedUrl).searchParams;
    expect(params.get("workspaceId")).toBe("branch-1");
    expect(params.get("intentKind")).toBe("ontology,view");
    expect(params.get("authorType")).toBe("human,system,unknown");
    expect(params.get("traceStatus")).toBe("traceable");
  });

  it("serializes scalar filter params (TC-ILAPI-002)", async () => {
    let capturedUrl = "";
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        capturedUrl = request.url;
        return HttpResponse.json({ data: SAMPLE });
      }),
    );
    const { result } = renderHook(
      () =>
        useIntentLog("branch-1", {
          q: "audit",
          timeRange: "custom",
          from: "2026-05-01",
          to: "2026-06-01",
          repo: "g5",
          sort: "actor",
          group: "day",
        }),
      { wrapper: makeWrapper() },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));
    const params = new URL(capturedUrl).searchParams;
    expect(params.get("q")).toBe("audit");
    expect(params.get("timeRange")).toBe("custom");
    expect(params.get("from")).toBe("2026-05-01");
    expect(params.get("to")).toBe("2026-06-01");
    expect(params.get("repo")).toBe("g5");
    expect(params.get("sort")).toBe("actor");
    expect(params.get("group")).toBe("day");
  });

  it("is disabled (stays loading, no fetch) when branch id is empty (TC-ILAPI-003)", async () => {
    let called = false;
    server.use(
      http.get("*/api/v1/intent-log", () => {
        called = true;
        return HttpResponse.json({ data: SAMPLE });
      }),
    );
    const { result } = renderHook(() => useIntentLog("", {}), {
      wrapper: makeWrapper(),
    });
    // Give react-query a tick; it must not fire the query.
    await new Promise((r) => setTimeout(r, 50));
    expect(result.current.status).toBe("loading");
    expect(called).toBe(false);
  });
});
