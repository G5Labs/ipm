import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { describe, expect, it } from "vitest";
import { server } from "../test/msw/server";
import {
  useTraceParents,
  type TraceRequest,
  type TraceResponse,
} from "./useGrounding";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  return { wrapper, queryClient };
}

const traceRequest: TraceRequest = {
  workspaceId: "branch-code",
  repoId: "repo-code",
  fileType: "ontology",
  fileName: ".semlift/specs/example.semlift.md",
  intentRecordNumber: "I1",
};

describe("useGrounding", () => {
  it("round-trips additive trace fields without changing the request", async () => {
    let requestedUrl = "";
    const response: TraceResponse = {
      totalCount: 1,
      results: [
        {
          intentRecordId: "trace-1",
          workspaceId: "branch-chat",
          repoId: "repo-docs",
          fileType: "chat",
          fileName: null,
          intentRecordNumber: "abcd1234",
          codeBlockName: null,
          content: "Chat context: Context\n---------\nUser input: Hello",
          chatSessionId: "session-1",
          chatContext: "Context",
          userContent: "Hello",
          createdBy: "human@example.com",
          createdAt: "2026-08-03T12:34:56Z",
          sourceFilePath: null,
          isHuman: true,
          priority: null,
          certainty: null,
        },
      ],
    };
    server.use(
      http.get("*/api/v1/grounding/trace-parents", ({ request }) => {
        requestedUrl = request.url;
        return HttpResponse.json({ data: response });
      }),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useTraceParents(traceRequest), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status !== "ok") throw new Error("trace request failed");
    expect(result.current.data).toEqual(response);
    const params = new URL(requestedUrl).searchParams;
    expect(Object.fromEntries(params.entries())).toEqual({
      workspaceId: "branch-code",
      repoId: "repo-code",
      fileType: "ontology",
      fileName: ".semlift/specs/example.semlift.md",
      intentRecordNumber: "I1",
    });
  });

  it("accepts a rolling-deployment response without additive fields", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json({
          data: {
            totalCount: 1,
            results: [
              {
                intentRecordId: "legacy",
                workspaceId: "branch-code",
                repoId: "repo-code",
                fileType: "ontology",
                fileName: ".semlift/specs/example.semlift.md",
                intentRecordNumber: "I1",
                codeBlockName: null,
                content: "Legacy body",
                isHuman: false,
                priority: null,
                certainty: null,
              },
            ],
          },
        }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useTraceParents(traceRequest), {
      wrapper,
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status !== "ok") throw new Error("trace request failed");
    expect(result.current.data.results[0].content).toBe("Legacy body");
    expect(result.current.data.results[0].sourceFilePath).toBeUndefined();
  });
});
