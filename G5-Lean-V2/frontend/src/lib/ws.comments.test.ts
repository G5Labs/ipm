import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { WebSocketManager, type WSEvent } from "./ws";

// Live anchored-comment changes (`g5.documents.comments.live`): every list
// holding a thread is read again after it changes elsewhere, a first read
// still out included.

function receive(queryClient: QueryClient, event: WSEvent) {
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient,
  });
  (manager as unknown as { handleEvent: (value: WSEvent) => void }).handleEvent(
    event,
  );
}

/** A list on screen; each of its reads answers when the test says. */
function watched(queryClient: QueryClient, queryKey: readonly unknown[]) {
  const answers: Array<(value: string[]) => void> = [];
  const observer = new QueryObserver<string[]>(queryClient, {
    queryKey,
    queryFn: () =>
      new Promise<string[]>((resolve) => {
        answers.push(resolve);
      }),
  });
  const unsubscribe = observer.subscribe(() => {});
  return { observer, answers, unsubscribe };
}

const events: Record<string, WSEvent> = {
  "comment.new": {
    type: "comment.new",
    workspaceId: "ws-1",
    commentId: "comment-2",
    scope: "line",
    nodeId: "node-1",
    originBranch: "main",
  },
  "comment.reply": {
    type: "comment.reply",
    workspaceId: "ws-1",
    commentId: "comment-1",
    replyId: "reply-1",
    authorName: "Rita",
  },
  "comment.updated": {
    type: "comment.updated",
    workspaceId: "ws-1",
    commentId: "comment-1",
    resolved: true,
  },
  "comment.deleted": {
    type: "comment.deleted",
    workspaceId: "ws-1",
    commentId: "comment-1",
  },
};

describe("anchored comment events", () => {
  it.each([
    ...Object.keys(events).map((type) => ({
      type,
      list: "a file's threads",
      key: ["line-comments", "ws-1", "repo-1", "src/main.ts"],
    })),
    ...Object.keys(events).map((type) => ({
      type,
      list: "a node's threads",
      key: ["comments", "ws-1", "node-1"],
    })),
    ...["workspace-activity", "workspace-review-comments"].flatMap((root) =>
      Object.keys(events).map((type) => ({
        type,
        list: root,
        key: [root, "ws-1"],
      })),
    ),
    {
      type: "comment.reply",
      list: "the timeline thread's replies",
      key: ["workspace-review-replies", "comment-1"],
    },
    {
      type: "comment.reply",
      list: "the thread's replies",
      key: ["comment-replies", "comment-1"],
    },
  ])(
    "reads $list again after $type, though the first read was still out",
    async ({ type, key }) => {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const list = watched(queryClient, key);
      await vi.waitFor(() => expect(list.answers).toHaveLength(1));

      receive(queryClient, events[type]);
      // Another read goes out after the change.
      await vi.waitFor(() => expect(list.answers).toHaveLength(2));
      list.answers[1](["after"]);
      await vi.waitFor(() =>
        expect(list.observer.getCurrentResult().data).toEqual(["after"]),
      );
      // The read from before the change answers late, and changes nothing.
      list.answers[0](["before"]);
      await new Promise((resolve) => setTimeout(resolve, 10));
      expect(list.observer.getCurrentResult().data).toEqual(["after"]);
      list.unsubscribe();
    },
  );

  it("drops the deleted thread's replies", async () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(["comment-replies", "comment-1"], []);
    queryClient.setQueryData(["comment-replies", "comment-9"], []);
    receive(queryClient, events["comment.deleted"]);
    expect(
      queryClient.getQueryState(["comment-replies", "comment-1"]),
    ).toBeUndefined();
    expect(
      queryClient.getQueryState(["comment-replies", "comment-9"]),
    ).toBeDefined();
  });

  it("marks the workspace's loaded thread lists stale on a change", async () => {
    const queryClient = new QueryClient();
    const lists = [
      ["line-comments", "ws-1", "repo-1", "src/main.ts"],
      ["comments", "ws-1", "node-1"],
    ];
    for (const key of lists) queryClient.setQueryData(key, []);
    queryClient.setQueryData(["line-comments", "ws-2", "repo-1", "a.ts"], []);
    receive(queryClient, events["comment.updated"]);
    await vi.waitFor(() => {
      for (const key of lists) {
        expect(queryClient.getQueryState(key)?.isInvalidated).toBe(true);
      }
    });
    expect(
      queryClient.getQueryState(["line-comments", "ws-2", "repo-1", "a.ts"])
        ?.isInvalidated,
    ).toBe(false);
  });
});
