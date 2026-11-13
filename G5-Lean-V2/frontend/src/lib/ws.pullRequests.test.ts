import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { expect, it, vi } from "vitest";
import { WebSocketManager, type WSEvent } from "./ws";

it.each(["source", "target", "all"])(
  "reads current %s PR evidence after an event overtakes the first request",
  async (role) => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    });
    const answers: Array<(value: string[]) => void> = [];
    const observer = new QueryObserver(queryClient, {
      queryKey: ["workspace-pull-requests", "ws-1", role],
      queryFn: () => new Promise<string[]>((resolve) => answers.push(resolve)),
    });
    const unsubscribe = observer.subscribe(() => {});
    const unrelated = ["workspace-pull-requests", "ws-2", role];
    queryClient.setQueryData(unrelated, ["unrelated"]);
    const manager = new WebSocketManager({
      getBaseUrl: () => "ws://test",
      getWsTicket: async () => "ticket",
      queryClient,
    });
    try {
      await vi.waitFor(() => expect(answers).toHaveLength(1));
      (
        manager as unknown as { handleEvent: (event: WSEvent) => void }
      ).handleEvent({
        type: "workspace.pull_request_updated",
        workspaceId: "ws-1",
        pullRequestId: "pr-1",
      });
      await vi.waitFor(() => expect(answers).toHaveLength(2));
      answers[1](["current"]);
      await vi.waitFor(() =>
        expect(observer.getCurrentResult().data).toEqual(["current"]),
      );
      answers[0](["old"]);
      await new Promise((resolve) => setTimeout(resolve, 10));
      expect(observer.getCurrentResult().data).toEqual(["current"]);
      expect(queryClient.getQueryState(unrelated)?.isInvalidated).toBe(false);
      expect(queryClient.getQueryData(unrelated)).toEqual(["unrelated"]);
    } finally {
      unsubscribe();
      manager.destroy();
      queryClient.clear();
    }
  },
);

it("refreshes the workspace's Activity when its pull request changes (g5.git.pr-merge-announcement)", async () => {
  const queryClient = new QueryClient();
  queryClient.setQueryData(["workspace-activity", "ws-1"], { entries: [] });
  queryClient.setQueryData(["workspace-activity", "ws-2"], { entries: [] });
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient,
  });
  try {
    (
      manager as unknown as { handleEvent: (event: WSEvent) => void }
    ).handleEvent({
      type: "workspace.pull_request_updated",
      workspaceId: "ws-1",
      pullRequestId: "pr-1",
    });
    await vi.waitFor(() =>
      expect(
        queryClient.getQueryState(["workspace-activity", "ws-1"])
          ?.isInvalidated,
      ).toBe(true),
    );
    expect(
      queryClient.getQueryState(["workspace-activity", "ws-2"])?.isInvalidated,
    ).toBe(false);
  } finally {
    manager.destroy();
    queryClient.clear();
  }
});
