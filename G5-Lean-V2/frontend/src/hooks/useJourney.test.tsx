// =============================================================================
// useJourney — marks bound to their workspace; loaded marks survive a
// transient refresh failure (g5.workbench.journey.marks)
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import { useJourneyMarks, useSetJourneyMark } from "./useJourney";

function wrapperFor(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe("useSetJourneyMark", () => {
  it("writes to the workspace bound at mutate time, not the hook's current one", async () => {
    const puts: string[] = [];
    server.use(
      http.put(
        "/api/v1/workspaces/:id/journey/steps/:stepId",
        async ({ params }) => {
          puts.push(`${params.id}/${params.stepId}`);
          await new Promise((resolve) => setTimeout(resolve, 20));
          return HttpResponse.json({ data: { marks: [] } });
        },
      ),
    );
    const queryClient = new QueryClient({
      defaultOptions: {
        mutations: { retry: false },
        queries: { retry: false },
      },
    });
    const { result } = renderHook(() => useSetJourneyMark(), {
      wrapper: wrapperFor(queryClient),
    });
    let settled: Promise<unknown> | null = null;
    act(() => {
      settled = result.current.mutateAsync({
        workspaceId: "ws-a",
        stepId: "se-plan",
        done: true,
      });
    });
    await act(async () => {
      await settled;
    });
    expect(puts).toEqual(["ws-a/se-plan"]);
  });
});

describe("useJourneyMarks", () => {
  it("keeps the loaded marks through a transient refresh failure", async () => {
    let fail = false;
    server.use(
      http.get("/api/v1/workspaces/:id/journey", () =>
        fail
          ? HttpResponse.json(
              { error: { code: "INTERNAL", message: "down", details: {} } },
              { status: 503 },
            )
          : HttpResponse.json({
              data: {
                marks: [
                  {
                    stepId: "se-plan",
                    done: true,
                    markedAt: "2026-10-03T00:00:00Z",
                  },
                ],
              },
            }),
      ),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { result } = renderHook(() => useJourneyMarks("ws-a", true), {
      wrapper: wrapperFor(queryClient),
    });
    await waitFor(() => expect(result.current.status).toBe("ok"));
    fail = true;
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: ["journey", "ws-a"] });
    });
    await waitFor(() => {
      expect(result.current.status).toBe("ok");
      if (result.current.status === "ok") {
        expect(result.current.refreshError).not.toBeNull();
        expect(result.current.data.map((mark) => mark.stepId)).toEqual([
          "se-plan",
        ]);
      }
    });
  });
});
