import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { describe, expect, it } from "vitest";
import type { CommentDTO } from "../lib/types";
import { server } from "../test/msw/server";
import {
  useAddComment,
  useCommentReplies,
  useDeleteComment,
  useFileComments,
  useReplyToComment,
  useResolveComment,
} from "./useComments";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
  return { queryClient, wrapper };
}

const comment: CommentDTO = {
  id: "comment-1",
  workspaceId: "workspace-1",
  repoId: null,
  authorId: "user-1",
  authorName: "Alice",
  authorAvatarUrl: null,
  scope: "node",
  filePath: null,
  lineNumber: 2,
  commitSha: null,
  content: "Review this",
  resolved: false,
  resolvedAt: null,
  originBranch: "main",
  replyCount: 1,
  createdAt: "2026-08-04T00:00:00Z",
  updatedAt: "2026-08-04T00:00:00Z",
};

describe("useDeleteComment", () => {
  it("removes the thread from comment caches after API deletion", async () => {
    server.use(
      http.delete("/api/v1/comments/:commentId", () =>
        HttpResponse.json({ data: { deleted: true } }),
      ),
    );
    const { queryClient, wrapper } = createWrapper();
    const nodeKey = ["comments", "workspace-1", "node-1"];
    const lineKey = ["line-comments", "workspace-1", "file.md", "main"];
    queryClient.setQueryData(nodeKey, [comment]);
    queryClient.setQueryData(lineKey, [comment]);
    const { result } = renderHook(() => useDeleteComment(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({ commentId: comment.id });
    });

    expect(queryClient.getQueryData(nodeKey)).toEqual([]);
    expect(queryClient.getQueryData(lineKey)).toEqual([]);
  });

  it("keeps the thread listed while deleting, and when deletion is refused", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.delete("/api/v1/comments/:commentId", async () => {
        await gate;
        return HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "Only the workspace owner can delete this thread",
              requestId: "request-1",
            },
          },
          { status: 403 },
        );
      }),
    );
    const { queryClient, wrapper } = createWrapper();
    const nodeKey = ["comments", "workspace-1", "node-1"];
    queryClient.setQueryData(nodeKey, [comment]);
    const { result } = renderHook(() => useDeleteComment(), { wrapper });

    let settled: Promise<unknown> | undefined;
    act(() => {
      settled = result.current.mutateAsync({ commentId: comment.id }).then(
        () => "deleted",
        (error: unknown) => error,
      );
    });
    // Still listed while the request runs: the card is never unmounted.
    await waitFor(() => expect(result.current.isLoading).toBe(true));
    expect(queryClient.getQueryData(nodeKey)).toEqual([comment]);
    let outcome: unknown;
    await act(async () => {
      release();
      outcome = await settled;
    });
    expect(outcome).toBeInstanceOf(Error);
    expect(queryClient.getQueryData(nodeKey)).toEqual([comment]);
  });
});

describe("useAddComment", () => {
  it("sends the file identity on creates, with no branch", async () => {
    // A file's threads are identified by workspace, repository and path; the
    // server labels the branch itself (g5.documents.comments.identity).
    let receivedBody: Record<string, unknown> | null = null;
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/comments",
        async ({ request }) => {
          receivedBody = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json(
            {
              data: {
                ...comment,
                id: "comment-2",
                scope: "line",
                nodeId: null,
                repoId: "repo-1",
                filePath: "docs/ws-1/plan.md",
                originBranch: "workspace/feature-x",
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(() => useAddComment(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({
        workspaceId: "workspace-1",
        scope: "line",
        content: "Doc comment",
        repoId: "repo-1",
        filePath: "docs/ws-1/plan.md",
        lineNumber: 3,
      });
    });

    expect(receivedBody).not.toBeNull();
    expect(receivedBody).not.toHaveProperty("originBranch");
    expect(receivedBody!.repoId).toBe("repo-1");
    expect(receivedBody!.filePath).toBe("docs/ws-1/plan.md");
  });
});

describe("useFileComments", () => {
  it("lists a file's threads by repository and path, with no branch filter", async () => {
    let requestedUrl: URL | null = null;
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/comments", ({ request }) => {
        requestedUrl = new URL(request.url);
        return HttpResponse.json({ data: [comment] });
      }),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useFileComments("workspace-1", "repo-1", "docs/plan.md"),
      { wrapper },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    expect(requestedUrl!.searchParams.get("repoId")).toBe("repo-1");
    expect(requestedUrl!.searchParams.get("filePath")).toBe("docs/plan.md");
    expect(requestedUrl!.searchParams.has("branch")).toBe(false);
  });
});

describe("useResolveComment", () => {
  it("keeps a thread as it was until the server accepts, and unchanged when refused", async () => {
    // The card stays mounted while the request runs, so a refusal can be
    // shown on it with any reply still typed (g5.documents.comments.failure).
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.patch("/api/v1/comments/:commentId", async () => {
        await gate;
        return HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "Only the workspace owner can resolve",
              requestId: "request-2",
            },
          },
          { status: 403 },
        );
      }),
    );
    const { queryClient, wrapper } = createWrapper();
    const fileKey = ["line-comments", "workspace-1", "repo-1", "docs/plan.md"];
    const fileComment = { ...comment, scope: "line", nodeId: null };
    queryClient.setQueryData(fileKey, [fileComment]);
    const { result } = renderHook(() => useResolveComment(), { wrapper });

    let settled: Promise<unknown> | undefined;
    act(() => {
      settled = result.current.mutateAsync({ commentId: comment.id }).then(
        () => "resolved",
        (error: unknown) => error,
      );
    });
    await waitFor(() => expect(result.current.isLoading).toBe(true));
    expect(queryClient.getQueryData(fileKey)).toEqual([fileComment]);
    let outcome: unknown;
    await act(async () => {
      release();
      outcome = await settled;
    });
    expect(outcome).toBeInstanceOf(Error);
    expect(queryClient.getQueryData(fileKey)).toEqual([fileComment]);
  });

  it("shows the thread resolved once the server accepts", async () => {
    server.use(
      http.patch("/api/v1/comments/:commentId", () =>
        HttpResponse.json({ data: { ...comment, resolved: true } }),
      ),
    );
    const { queryClient, wrapper } = createWrapper();
    const fileKey = ["line-comments", "workspace-1", "repo-1", "docs/plan.md"];
    queryClient.setQueryData(fileKey, [comment]);
    const { result } = renderHook(() => useResolveComment(), { wrapper });

    await act(async () => {
      await result.current.mutateAsync({ commentId: comment.id });
    });
    expect(
      (queryClient.getQueryData(fileKey) as CommentDTO[])[0].resolved,
    ).toBe(true);
  });
});

// A change committed here while a list's first read is still out: that read,
// from before the change, does not end the list's reading again
// (`g5.documents.comments.live`).
describe("a list whose first read is still out", () => {
  function heldFirstRead<T>(before: T, after: T) {
    let reads = 0;
    let answerFirst!: () => void;
    const firstHeld = new Promise<void>((resolve) => {
      answerFirst = resolve;
    });
    const answer = async () => {
      reads += 1;
      if (reads === 1) {
        await firstHeld;
        return HttpResponse.json({ data: before });
      }
      return HttpResponse.json({ data: after });
    };
    return { answer, answerFirst, reads: () => reads };
  }

  it("shows a thread created here while the file's first read is out", async () => {
    const created: CommentDTO = {
      ...comment,
      id: "comment-2",
      scope: "line",
      repoId: "repo-1",
      filePath: "src/main.ts",
    };
    const list = heldFirstRead<CommentDTO[]>([], [created]);
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/comments", list.answer),
      http.post("/api/v1/workspaces/:workspaceId/comments", () =>
        HttpResponse.json({ data: created }, { status: 201 }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        threads: useFileComments("workspace-1", "repo-1", "src/main.ts"),
        add: useAddComment(),
      }),
      { wrapper },
    );
    await waitFor(() => expect(list.reads()).toBe(1));

    await act(async () => {
      await result.current.add.mutateAsync({
        workspaceId: "workspace-1",
        scope: "line",
        content: "Split this?",
        repoId: "repo-1",
        filePath: "src/main.ts",
        lineNumber: 3,
      });
    });
    await act(async () => {
      list.answerFirst();
    });
    await waitFor(() =>
      expect(
        result.current.threads.status === "ok" &&
          result.current.threads.data.map((thread) => thread.id),
      ).toEqual(["comment-2"]),
    );
  });

  it("shows a reply sent here while the thread's first read of replies is out", async () => {
    const reply = {
      id: "reply-1",
      commentId: "comment-1",
      authorId: "user-1",
      authorName: "Alice",
      authorAvatarUrl: null,
      content: "Yes",
      createdAt: "2026-09-30T10:05:00Z",
    };
    const replies = heldFirstRead<unknown[]>([], [reply]);
    server.use(
      http.get("/api/v1/comments/:commentId/replies", replies.answer),
      http.post("/api/v1/comments/:commentId/replies", () =>
        HttpResponse.json({ data: reply }, { status: 201 }),
      ),
    );
    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => ({
        replies: useCommentReplies("comment-1"),
        send: useReplyToComment(),
      }),
      { wrapper },
    );
    await waitFor(() => expect(replies.reads()).toBe(1));

    await act(async () => {
      await result.current.send.mutateAsync({
        commentId: "comment-1",
        content: "Yes",
      });
    });
    await act(async () => {
      replies.answerFirst();
    });
    await waitFor(() =>
      expect(
        result.current.replies.status === "ok" &&
          result.current.replies.data.map((each) => each.id),
      ).toEqual(["reply-1"]),
    );
  });
});
