import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "../test/msw/server";
import type { ReviewSuggestion } from "../lib/types";
import { useFileSuggestions } from "../components/workspace/ReviewSuggestionOverlay";
import {
  useRejectReviewSuggestion,
  useReviewSuggestions,
} from "./useWorkspaceReview";

const workspace = "identity-workspace";
const filter = { repoId: "repo", path: "spec.md", currentOnly: true };
const key = ["workspace-review-suggestions", workspace, filter];
function suggestion(
  overrides: Partial<ReviewSuggestion> = {},
): ReviewSuggestion {
  return {
    id: "identity-parent",
    kind: "comment",
    status: "open",
    repoId: "repo",
    path: "spec.md",
    createdAt: "2026-10-01T12:00:00Z",
    anchorStart: 1,
    ...overrides,
  } as ReviewSuggestion;
}
function harness() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const view = renderHook(
    () => ({
      state: useReviewSuggestions(workspace, { ...filter }),
      file: useFileSuggestions(workspace, "repo", "spec.md", true),
      reject: useRejectReviewSuggestion(workspace),
    }),
    { wrapper },
  );
  return { ...view, queryClient };
}

describe("review suggestion state identity through the file consumer", () => {
  it("retains wrapper and file list identity across unchanged loading and loaded parent renders, and updates data", async () => {
    let resolve!: () => void;
    const pending = new Promise<void>((done) => {
      resolve = done;
    });
    server.use(
      http.get(
        `/api/v1/workspaces/${workspace}/review/suggestions`,
        async () => {
          await pending;
          return HttpResponse.json({ data: { suggestions: [suggestion()] } });
        },
      ),
    );
    const view = harness();
    const loading = view.result.current.state;
    const loadingFile = view.result.current.file;
    expect(loading.status).toBe("loading");
    view.rerender();
    expect.soft(view.result.current.state).toBe(loading);
    expect.soft(view.result.current.file).toBe(loadingFile);
    resolve();
    await waitFor(() => expect(view.result.current.state.status).toBe("ok"));
    expect(view.result.current.state).not.toBe(loading);
    const loaded = view.result.current.state;
    const loadedFile = view.result.current.file;
    view.rerender();
    expect.soft(view.result.current.state).toBe(loaded);
    expect.soft(view.result.current.file).toBe(loadedFile);
    expect
      .soft(view.result.current.file.suggestions)
      .toBe(loadedFile.suggestions);
    act(() =>
      view.queryClient.setQueryData(key, {
        suggestions: [suggestion({ note: "New note" })],
      }),
    );
    await waitFor(() =>
      expect(view.result.current.file.suggestions[0]?.note).toBe("New note"),
    );
    expect(view.result.current.state).not.toBe(loaded);
    expect(view.result.current.file).not.toBe(loadedFile);
  });

  it("updates transient refresh errors, recovery and access refusal without dropping loaded rows during a server failure", async () => {
    let status = 200;
    server.use(
      http.get(`/api/v1/workspaces/${workspace}/review/suggestions`, () =>
        status === 200
          ? HttpResponse.json({ data: { suggestions: [suggestion()] } })
          : HttpResponse.json(
              { error: { code: "unavailable", message: "Refresh refused" } },
              { status },
            ),
      ),
    );
    const view = harness();
    await waitFor(() => expect(view.result.current.state.status).toBe("ok"));
    const loaded = view.result.current.state;
    const loadedRows = view.result.current.file.suggestions;
    status = 503;
    await act(async () => {
      await view.queryClient.refetchQueries({ queryKey: key });
    });
    await waitFor(() => {
      const state = view.result.current.state;
      expect(state.status).toBe("ok");
      if (state.status === "ok") expect(state.refreshError).not.toBeNull();
    });
    expect(view.result.current.state).not.toBe(loaded);
    expect(view.result.current.file.suggestions).toEqual(loadedRows);
    expect(view.result.current.file.problem).toMatch(/Could not refresh/);
    const failed = view.result.current.state;
    const failedFile = view.result.current.file;
    view.rerender();
    expect.soft(view.result.current.state).toBe(failed);
    expect.soft(view.result.current.file).toBe(failedFile);
    status = 200;
    await act(async () => {
      await view.queryClient.refetchQueries({ queryKey: key });
    });
    await waitFor(() => expect(view.result.current.file.problem).toBeNull());
    expect(view.result.current.state).not.toBe(failed);
    status = 403;
    await act(async () => {
      await view.queryClient.refetchQueries({ queryKey: key });
    });
    await waitFor(() => expect(view.result.current.state.status).toBe("error"));
    expect(view.result.current.file.suggestions).toEqual([]);
    expect(view.result.current.file.problem).toMatch(/Could not load/);
  });

  it("updates settled-memory rows from a real decision despite a stale list refresh", async () => {
    const parent = suggestion();
    server.use(
      http.get(`/api/v1/workspaces/${workspace}/review/suggestions`, () =>
        HttpResponse.json({ data: { suggestions: [parent] } }),
      ),
      http.post(
        `/api/v1/workspaces/${workspace}/review/suggestions/${parent.id}/reject`,
        () =>
          HttpResponse.json({
            data: {
              suggestion: {
                ...parent,
                status: "rejected",
                statusAt: "2026-10-01T12:10:00Z",
              },
              writtenHash: null,
              alreadyApplied: false,
            },
          }),
      ),
    );
    const view = harness();
    await waitFor(() =>
      expect(view.result.current.file.suggestions).toHaveLength(1),
    );
    const loaded = view.result.current.state;
    const loadedFile = view.result.current.file;
    await act(async () => {
      await view.result.current.reject.mutateAsync({ suggestionId: parent.id });
    });
    await waitFor(() => {
      const state = view.result.current.state;
      if (state.status !== "ok") throw new Error("Waiting for settled state");
      expect(state.data.suggestions[0].status).toBe("rejected");
      expect(view.result.current.file.suggestions).toEqual([]);
    });
    expect(view.result.current.state).not.toBe(loaded);
    expect(view.result.current.file).not.toBe(loadedFile);
    await act(async () => {
      await view.queryClient.refetchQueries({ queryKey: key });
    });
    const decided = view.result.current.state;
    const decidedFile = view.result.current.file;
    view.rerender();
    expect.soft(view.result.current.state).toBe(decided);
    expect.soft(view.result.current.file).toBe(decidedFile);
    if (view.result.current.state.status === "ok")
      expect(view.result.current.state.data.suggestions[0].status).toBe(
        "rejected",
      );
  });
});
