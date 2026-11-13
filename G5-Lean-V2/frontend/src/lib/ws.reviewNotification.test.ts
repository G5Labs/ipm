import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { WebSocketManager, type WSEvent } from "./ws";

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

async function invalidatedHeads(event: WSEvent): Promise<string[]> {
  const queryClient = new QueryClient();
  const invalidateQueries = vi.spyOn(queryClient, "invalidateQueries");
  receive(queryClient, event);
  // Some lists are read again after a first read is cancelled.
  await new Promise((resolve) => setTimeout(resolve, 0));
  return invalidateQueries.mock.calls
    .map((call) => (call[0] as { queryKey?: unknown[] })?.queryKey?.[0])
    .filter((value): value is string => typeof value === "string");
}

const reviewNotice: WSEvent = {
  type: "notification",
  workspaceId: "workspace-1",
  notification: {
    id: "n-1",
    category: "review",
    title: "kaihui-test moved on",
  },
};

describe("review notifications", () => {
  it("refreshes the review metadata a reviewer's copy shows, never its preview", async () => {
    const heads = await invalidatedHeads(reviewNotice);
    expect(heads).toEqual(
      expect.arrayContaining([
        "notifications",
        "workspace-review-publications",
        "workspace-review-suggestions",
        "workspace-review-suggestion-counts",
        "workspace-changes",
        // A reply or decision notice shows in the open thread at once.
        "workspace-review-replies",
      ]),
    );
    expect(heads).not.toContain("workspace-review-preview");
  });

  it("leaves review queries alone for other notification categories", async () => {
    expect(
      await invalidatedHeads({
        type: "notification",
        workspaceId: "workspace-1",
        notification: { id: "n-2", category: "comment" },
      }),
    ).toEqual(["notifications"]);
  });

  it("reads an open discussion again after a review notice, though its first read was still out", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const answers: Array<(value: string[]) => void> = [];
    const discussion = new QueryObserver<string[]>(queryClient, {
      queryKey: ["workspace-review-replies", "workspace-1", "sug-1"],
      queryFn: () =>
        new Promise<string[]>((resolve) => {
          answers.push(resolve);
        }),
    });
    const unsubscribe = discussion.subscribe(() => {});
    await vi.waitFor(() => expect(answers).toHaveLength(1));

    receive(queryClient, reviewNotice);
    await vi.waitFor(() => expect(answers).toHaveLength(2));
    answers[1](["after"]);
    await vi.waitFor(() =>
      expect(discussion.getCurrentResult().data).toEqual(["after"]),
    );
    // The read from before the notice answers late, and changes nothing.
    answers[0](["before"]);
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(discussion.getCurrentResult().data).toEqual(["after"]);
    unsubscribe();
  });
});
