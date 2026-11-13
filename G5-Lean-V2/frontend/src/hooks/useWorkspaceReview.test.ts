/**
 * useWorkspaceReview — a discard writes into the copy's working tree, so
 * every reader of that tree reloads: the preview, the file viewer, the
 * document editor (which needs the new content hash to save), the tree
 * and the changes report. A decision's answer is shown by every list
 * already read (`g5.governance.review-suggestions.display`).
 */

import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../test/msw/server";
import type { ReviewSuggestion } from "../lib/types";
import { reviewEditFixture } from "../test/reviewEdits";
import {
  useAddReviewSuggestionReply,
  useDiscardReviewChanges,
  useRejectReviewSuggestion,
  useReviewSuggestionReplies,
  useReviewSuggestions,
  useWithdrawReviewSuggestion,
} from "./useWorkspaceReview";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  return { wrapper, queryClient };
}

describe("durable child decisions in shared suggestion queries", () => {
  it.each(["accepted", "rejected"])(
    "withdraws the parent without an edit selector and preserves a %s child's disposition and discussion after a stale refresh",
    async (settledStatus) => {
      const content = "Alpha middle Omega.\n";
      const parent = {
        id: "withdraw-parent",
        kind: "hunk",
        status: "open",
        beforeLines: content,
        commentId: "original-discussion",
        edits: [
          reviewEditFixture("alpha", content, 0, 5, "Beta"),
          reviewEditFixture("settled", content, 6, 12, "changed", {
            status: settledStatus,
            statusAt: "2026-10-01T16:00:00Z",
            commentId: "settled-discussion",
          }),
          reviewEditFixture("omega", content, 13, 18, "Gamma"),
        ],
      } as ReviewSuggestion;
      const writes: unknown[] = [];
      server.use(
        http.get("/api/v1/workspaces/src-withdraw/review/suggestions", () =>
          HttpResponse.json({ data: { suggestions: [parent] } }),
        ),
        http.post(
          "/api/v1/workspaces/src-withdraw/review/suggestions/withdraw-parent/withdraw",
          async ({ request }) => {
            writes.push(await request.json());
            return HttpResponse.json({
              data: {
                suggestion: {
                  ...parent,
                  status: "withdrawn",
                  statusAt: "2026-10-01T16:10:00Z",
                  edits: parent.edits!.map((edit) =>
                    edit.status === "open"
                      ? {
                          ...edit,
                          status: "withdrawn",
                          statusAt: "2026-10-01T16:10:00Z",
                        }
                      : edit,
                  ),
                },
                writtenHash: null,
                alreadyApplied: false,
              },
            });
          },
        ),
      );
      const { wrapper, queryClient } = createWrapper();
      const { result } = renderHook(
        () => ({
          all: useReviewSuggestions("src-withdraw"),
          open: useReviewSuggestions("src-withdraw", { openOnly: true }),
          withdraw: useWithdrawReviewSuggestion("src-withdraw"),
        }),
        { wrapper },
      );
      await waitFor(() => expect(result.current.open.status).toBe("ok"));
      await act(async () => {
        await result.current.withdraw.mutateAsync({
          suggestionId: "withdraw-parent",
          editId: "alpha",
        });
        await queryClient.invalidateQueries({
          queryKey: ["workspace-review-suggestions", "src-withdraw"],
        });
      });
      expect(writes).toEqual([{}]);
      await waitFor(() => {
        if (
          result.current.all.status !== "ok" ||
          result.current.open.status !== "ok"
        )
          throw new Error("Waiting for withdrawal");
        expect(result.current.open.data.suggestions).toEqual([]);
        const rows = result.current.all.data.suggestions;
        expect(rows.map((row) => row.status)).toEqual([
          "withdrawn",
          settledStatus,
          "withdrawn",
        ]);
        expect(rows[1].commentId).toBe("settled-discussion");
        expect(
          rows.every((row) => row.parentCommentId === "original-discussion"),
        ).toBe(true);
        expect(rows[1].reviewEdit).toEqual(parent.edits![1]);
      });
    },
  );

  it("separates historical and current publication instances of the same carried edit UUID", async () => {
    const content = "Alpha unchanged.\n";
    const edit = reviewEditFixture("carried-unit", content, 0, 5, "Beta");
    const old = {
      id: "old-publication-parent",
      kind: "hunk",
      status: "superseded",
      statusAt: "2026-10-01T16:10:00Z",
      edits: [edit],
    } as ReviewSuggestion;
    const current = {
      ...old,
      id: "current-publication-parent",
      status: "open",
      statusAt: null,
    };
    server.use(
      http.get("/api/v1/workspaces/src-continuation/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [old, current] } }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        all: useReviewSuggestions("src-continuation"),
        open: useReviewSuggestions("src-continuation", { openOnly: true }),
      }),
      { wrapper },
    );
    await waitFor(() => {
      if (
        result.current.all.status !== "ok" ||
        result.current.open.status !== "ok"
      )
        throw new Error("Waiting for lists");
      expect(
        result.current.all.data.suggestions.map(
          (row) => `${row.id}:${row.status}`,
        ),
      ).toEqual([
        "old-publication-parent:carried-unit:superseded",
        "current-publication-parent:carried-unit:open",
      ]);
      expect(result.current.open.data.suggestions.map((row) => row.id)).toEqual(
        ["current-publication-parent:carried-unit"],
      );
    });
  });

  it("does not reopen a decided child from a stale parent ledger and keeps its sibling open", async () => {
    const content = "Alpha unchanged middle clause Omega.\n";
    const parent = {
      id: "parent-ledger",
      kind: "hunk",
      status: "open",
      beforeLines: content,
      afterLines: content.replace("Alpha", "Beta").replace("Omega", "Gamma"),
      edits: [
        reviewEditFixture("child-alpha", content, 0, 5, "Beta"),
        reviewEditFixture(
          "child-omega",
          content,
          content.indexOf("Omega"),
          content.indexOf("Omega") + 5,
          "Gamma",
        ),
      ],
    } as ReviewSuggestion;
    const requests: unknown[] = [];
    server.use(
      http.get("/api/v1/workspaces/src-ledger/review/suggestions", () =>
        HttpResponse.json({ data: { suggestions: [parent] } }),
      ),
      http.post(
        "/api/v1/workspaces/src-ledger/review/suggestions/parent-ledger/reject",
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: {
                ...parent,
                edits: parent.edits!.map((edit) =>
                  edit.id === "child-alpha"
                    ? {
                        ...edit,
                        status: "rejected",
                        statusAt: "2026-10-01T16:10:00Z",
                      }
                    : edit,
                ),
              },
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => ({
        all: useReviewSuggestions("src-ledger"),
        open: useReviewSuggestions("src-ledger", { openOnly: true }),
        reject: useRejectReviewSuggestion("src-ledger"),
      }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.open.status).toBe("ok"));
    await act(async () => {
      await result.current.reject.mutateAsync({
        suggestionId: "parent-ledger",
        editId: "child-alpha",
        reason: "Keep original.",
      });
    });
    expect(requests).toEqual([
      { editId: "child-alpha", reason: "Keep original." },
    ]);
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["workspace-review-suggestions", "src-ledger"],
      });
    });
    await waitFor(() => {
      if (
        result.current.open.status !== "ok" ||
        result.current.all.status !== "ok"
      )
        throw new Error("Waiting for lists");
      expect(result.current.open.data.suggestions.map((row) => row.id)).toEqual(
        ["parent-ledger:child-omega"],
      );
      expect(
        result.current.all.data.suggestions.map(
          (row) => `${row.id}:${row.status}`,
        ),
      ).toEqual([
        "parent-ledger:child-alpha:rejected",
        "parent-ledger:child-omega:open",
      ]);
    });
  });
});

describe("useDiscardReviewChanges", () => {
  it("reloads the copy's documents and files after a discard", async () => {
    server.use(
      http.post("/api/v1/workspaces/:id/review/changes/discard", () =>
        HttpResponse.json({ data: { discarded: [], skipped: [] } }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    queryClient.setQueryData(
      ["workspace-file", "copy-1", "repo", "docs/design.md"],
      { content: "old" },
    );
    queryClient.setQueryData(["workspace-file", "copy-1", "a.rs"], "old");
    queryClient.setQueryData(["workspace-review-preview", "copy-1"], {});
    queryClient.setQueryData(["workspace-changes", "copy-1"], {});
    queryClient.setQueryData(["workspace-review-suggestions", "src-1"], {});

    const { result } = renderHook(
      () => useDiscardReviewChanges("copy-1", "src-1"),
      { wrapper },
    );
    await act(async () => {
      await result.current.mutateAsync({ keys: ["k1"] });
    });

    for (const key of [
      ["workspace-file", "copy-1", "repo", "docs/design.md"],
      ["workspace-file", "copy-1", "a.rs"],
      ["workspace-review-preview", "copy-1"],
      ["workspace-changes", "copy-1"],
      ["workspace-review-suggestions", "src-1"],
    ]) {
      expect(queryClient.getQueryState(key)?.isInvalidated).toBe(true);
    }
  });
});

describe("suggestion lists and decisions", () => {
  const SUGGESTIONS = "/api/v1/workspaces/:id/review/suggestions";
  const row = (
    id: string,
    status: ReviewSuggestion["status"],
    statusAt: string | null,
  ) => ({ id, status, statusAt, note: "Is beta needed?" }) as ReviewSuggestion;
  const shown = (state: { status: string; data?: unknown }) =>
    state.status === "ok"
      ? (state.data as { suggestions: ReviewSuggestion[] }).suggestions.map(
          (suggestion) => `${suggestion.id}:${suggestion.status}`,
        )
      : state.status;

  function answerReject(decided: ReviewSuggestion) {
    server.use(
      http.post(`${SUGGESTIONS}/:suggestionId/reject`, () =>
        HttpResponse.json({
          data: {
            suggestion: decided,
            writtenHash: null,
            alreadyApplied: false,
          },
        }),
      ),
    );
  }

  it("shows a decision's answer in every list of the source, and drops it from one narrowed to open rows", async () => {
    // The server keeps answering with what it read before the decision.
    server.use(
      http.get(SUGGESTIONS, ({ params, request }) => {
        const openOnly =
          new URL(request.url).searchParams.get("openOnly") === "true";
        const rows =
          params.id === "src-1"
            ? [row("sug-1", "open", null), row("sug-2", "open", null)]
            : [row("sug-1", "open", null)];
        return HttpResponse.json({
          data: {
            suggestions: openOnly
              ? rows.filter((suggestion) => suggestion.status === "open")
              : rows,
          },
        });
      }),
    );
    answerReject(row("sug-1", "rejected", "2026-09-30T11:00:00Z"));
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        every: useReviewSuggestions("src-1"),
        open: useReviewSuggestions("src-1", { openOnly: true }),
        elsewhere: useReviewSuggestions("src-2"),
        reject: useRejectReviewSuggestion("src-1"),
      }),
      { wrapper },
    );
    await waitFor(() =>
      expect(shown(result.current.elsewhere)).toEqual(["sug-1:open"]),
    );
    await waitFor(() => expect(shown(result.current.open)).toHaveLength(2));

    await act(async () => {
      await result.current.reject.mutateAsync({ suggestionId: "sug-1" });
    });

    await waitFor(() =>
      expect(shown(result.current.every)).toEqual([
        "sug-1:rejected",
        "sug-2:open",
      ]),
    );
    expect(shown(result.current.open)).toEqual(["sug-2:open"]);
    expect(shown(result.current.elsewhere)).toEqual(["sug-1:open"]);
  });

  it("never shows an answer over a later decision another list read", async () => {
    let everyReads = 0;
    server.use(
      http.get(SUGGESTIONS, ({ request }) => {
        if (new URL(request.url).searchParams.get("currentOnly") === "true") {
          return HttpResponse.json({
            data: { suggestions: [row("sug-1", "open", null)] },
          });
        }
        // Every round's list reads the later decision once, then fails.
        everyReads += 1;
        return everyReads === 1
          ? HttpResponse.json({
              data: {
                suggestions: [
                  row("sug-1", "withdrawn", "2026-09-30T12:00:00Z"),
                ],
              },
            })
          : HttpResponse.json(
              {
                error: {
                  code: "SERVICE_UNAVAILABLE",
                  message: "The service is restarting",
                  requestId: "request-down",
                },
              },
              { status: 503 },
            );
      }),
    );
    answerReject(row("sug-1", "rejected", "2026-09-30T11:00:00Z"));
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        current: useReviewSuggestions("src-1", { currentOnly: true }),
        every: useReviewSuggestions("src-1"),
        reject: useRejectReviewSuggestion("src-1"),
      }),
      { wrapper },
    );
    await waitFor(() =>
      expect(shown(result.current.current)).toEqual(["sug-1:withdrawn"]),
    );
    await act(async () => {
      await result.current.reject.mutateAsync({ suggestionId: "sug-1" });
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(shown(result.current.current)).toEqual(["sug-1:withdrawn"]);
    expect(shown(result.current.every)).toEqual(["sug-1:withdrawn"]);
  });

  it("shows a decision another view reads on a list that is not read again", async () => {
    let withdrawn = false;
    server.use(
      http.get(SUGGESTIONS, ({ request }) =>
        HttpResponse.json({
          data: {
            suggestions: [
              withdrawn &&
              new URL(request.url).searchParams.get("currentOnly") !== "true"
                ? row("sug-1", "withdrawn", "2026-09-30T11:00:00Z")
                : row("sug-1", "open", null),
            ],
          },
        }),
      ),
    );
    const { wrapper, queryClient } = createWrapper();
    // Two views, each its own component: the file's list stays as read.
    const file = renderHook(
      () => useReviewSuggestions("src-1", { currentOnly: true }),
      { wrapper },
    );
    const panel = renderHook(() => useReviewSuggestions("src-1"), {
      wrapper,
    });
    await waitFor(() =>
      expect(shown(file.result.current)).toEqual(["sug-1:open"]),
    );
    await waitFor(() =>
      expect(shown(panel.result.current)).toEqual(["sug-1:open"]),
    );

    withdrawn = true;
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["workspace-review-suggestions", "src-1", {}],
        exact: true,
      });
    });
    await waitFor(() =>
      expect(shown(panel.result.current)).toEqual(["sug-1:withdrawn"]),
    );
    await waitFor(() =>
      expect(shown(file.result.current)).toEqual(["sug-1:withdrawn"]),
    );
  });

  it("shows a decision on a list whose first read went out before it and is answered after it", async () => {
    let releaseFirstRead!: () => void;
    const firstReadHeld = new Promise<void>((resolve) => {
      releaseFirstRead = resolve;
    });
    server.use(
      http.get(SUGGESTIONS, async ({ request }) => {
        // Every round's list is slow to answer, with what it read first.
        if (new URL(request.url).searchParams.get("currentOnly") !== "true") {
          await firstReadHeld;
        }
        return HttpResponse.json({
          data: { suggestions: [row("sug-1", "open", null)] },
        });
      }),
    );
    answerReject(row("sug-1", "rejected", "2026-09-30T11:00:00Z"));
    const { wrapper } = createWrapper();
    const file = renderHook(
      () => ({
        current: useReviewSuggestions("src-1", { currentOnly: true }),
        reject: useRejectReviewSuggestion("src-1"),
      }),
      { wrapper },
    );
    await waitFor(() =>
      expect(shown(file.result.current.current)).toEqual(["sug-1:open"]),
    );
    // Another view opens, and its first read goes out.
    const panel = renderHook(() => useReviewSuggestions("src-1"), {
      wrapper,
    });
    await act(async () => {
      await file.result.current.reject.mutateAsync({ suggestionId: "sug-1" });
    });
    await act(async () => {
      releaseFirstRead();
    });
    await waitFor(() =>
      expect(shown(panel.result.current)).toEqual(["sug-1:rejected"]),
    );
  });

  it("shows a decision one list read on another whose refresh fails, still saying it failed", async () => {
    let withdrawn = false;
    let currentDown = false;
    server.use(
      http.get(SUGGESTIONS, ({ request }) => {
        if (new URL(request.url).searchParams.get("currentOnly") === "true") {
          return currentDown
            ? HttpResponse.json(
                {
                  error: {
                    code: "SERVICE_UNAVAILABLE",
                    message: "The service is restarting",
                    requestId: "request-down",
                  },
                },
                { status: 503 },
              )
            : HttpResponse.json({
                data: { suggestions: [row("sug-1", "open", null)] },
              });
        }
        return HttpResponse.json({
          data: {
            suggestions: [
              withdrawn
                ? row("sug-1", "withdrawn", "2026-09-30T11:00:00Z")
                : row("sug-1", "open", null),
            ],
          },
        });
      }),
    );
    const { wrapper, queryClient } = createWrapper();
    const { result } = renderHook(
      () => ({
        current: useReviewSuggestions("src-1", { currentOnly: true }),
        every: useReviewSuggestions("src-1"),
      }),
      { wrapper },
    );
    await waitFor(() =>
      expect(shown(result.current.every)).toEqual(["sug-1:open"]),
    );
    await waitFor(() =>
      expect(shown(result.current.current)).toEqual(["sug-1:open"]),
    );

    // Withdrawn elsewhere: one list reads it, the other cannot be read.
    withdrawn = true;
    currentDown = true;
    await act(async () => {
      await queryClient.invalidateQueries();
    });
    await waitFor(() =>
      expect(shown(result.current.current)).toEqual(["sug-1:withdrawn"]),
    );
    const current = result.current.current;
    expect(current.status === "ok" && current.refreshError?.message).toBe(
      "The service is restarting",
    );
  });
});

// A discussion polls every ten seconds; a change made here is read at once,
// even while the discussion's first read is still out
// (`g5.governance.review-thread`).
describe("a discussion whose first read is still out", () => {
  const REPLIES =
    "/api/v1/workspaces/:id/review/suggestions/:suggestionId/replies";
  const reply = { id: "reply-1", content: "Yes" };

  function heldFirstRead() {
    let reads = 0;
    let answerFirst!: () => void;
    const firstHeld = new Promise<void>((resolve) => {
      answerFirst = resolve;
    });
    server.use(
      http.get(REPLIES, async () => {
        reads += 1;
        if (reads === 1) {
          await firstHeld;
          return HttpResponse.json({ data: { replies: [] } });
        }
        return HttpResponse.json({ data: { replies: [reply] } });
      }),
    );
    return { answerFirst, reads: () => reads };
  }

  const shownReplies = (state: { status: string; data?: unknown }) =>
    state.status === "ok"
      ? (state.data as { replies: Array<{ id: string }> }).replies.map(
          (each) => each.id,
        )
      : state.status;

  it("shows a reply sent here", async () => {
    const first = heldFirstRead();
    server.use(
      http.post(REPLIES, () =>
        HttpResponse.json({ data: { replies: [reply] } }, { status: 201 }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        replies: useReviewSuggestionReplies("src-1", "sug-1"),
        send: useAddReviewSuggestionReply("src-1"),
      }),
      { wrapper },
    );
    await waitFor(() => expect(first.reads()).toBe(1));
    await act(async () => {
      await result.current.send.mutateAsync({
        suggestionId: "sug-1",
        content: "Yes",
      });
    });
    await act(async () => {
      first.answerFirst();
    });
    await waitFor(() =>
      expect(shownReplies(result.current.replies)).toEqual(["reply-1"]),
    );
  });

  it("shows a decision's note made here", async () => {
    const first = heldFirstRead();
    server.use(
      http.post(`${REPLIES.replace("/replies", "")}/reject`, () =>
        HttpResponse.json({
          data: {
            suggestion: {
              id: "sug-1",
              status: "rejected",
              statusAt: "2026-09-30T11:00:00Z",
            },
            writtenHash: null,
            alreadyApplied: false,
          },
        }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        replies: useReviewSuggestionReplies("src-1", "sug-1"),
        reject: useRejectReviewSuggestion("src-1"),
      }),
      { wrapper },
    );
    await waitFor(() => expect(first.reads()).toBe(1));
    await act(async () => {
      await result.current.reject.mutateAsync({ suggestionId: "sug-1" });
    });
    await act(async () => {
      first.answerFirst();
    });
    await waitFor(() =>
      expect(shownReplies(result.current.replies)).toEqual(["reply-1"]),
    );
  });
});
