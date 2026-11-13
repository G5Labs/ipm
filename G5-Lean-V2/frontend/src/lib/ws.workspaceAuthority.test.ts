import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { WebSocketManager, type WSEvent } from "./ws";

function dispatch(event: WSEvent) {
  const invalidateQueries = vi.fn();
  const removeQueries = vi.fn();
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient: {
      invalidateQueries,
      removeQueries,
      getQueryData: vi.fn(),
      isFetching: vi.fn(() => 0),
      getQueryCache: () => ({ findAll: () => [] }),
    } as unknown as QueryClient,
  });
  (manager as unknown as { handleEvent: (value: WSEvent) => void }).handleEvent(
    event,
  );
  return { invalidateQueries, removeQueries };
}

function keyHeads(mock: ReturnType<typeof vi.fn>): string[] {
  return mock.mock.calls
    .map((call) => (call[0] as { queryKey?: unknown[] })?.queryKey?.[0])
    .filter((value): value is string => typeof value === "string");
}

describe("workspace durable-authority events", () => {
  it("refetches every source-dependent view after a completed flush", () => {
    const { invalidateQueries } = dispatch({
      type: "workspace.flush_completed",
      workspaceId: "workspace-1",
      flushId: "flush-1",
    });
    const heads = keyHeads(invalidateQueries);
    for (const key of [
      "workspace",
      "file-tree",
      "file-content",
      "docs",
      "workspace-changes",
    ]) {
      expect(heads).toContain(key);
    }
  });

  it.each([
    {
      type: "workspace.flush_completed",
      workspaceId: "workspace-1",
      flushId: "flush-1",
    },
    {
      type: "workspace.branch_updated",
      workspaceId: "workspace-1",
      repoId: "repo-1",
      branch: "main",
      headCommit: "abc123",
    },
  ] as WSEvent[])(
    "refreshes a reader's open FILES views when the durable source advances ($type)",
    (event) => {
      // `workspace.file_changed` reaches the owner alone; a reader's file
      // follows these (`g5.documents.files-view.refresh`).
      const { invalidateQueries } = dispatch(event);
      expect(invalidateQueries).toHaveBeenCalledWith({
        queryKey: ["workspace-file", "workspace-1"],
      });
      expect(invalidateQueries).toHaveBeenCalledWith({
        queryKey: ["source-control-change-graph", "workspace-1"],
      });
    },
  );

  it("drops live payloads and refetches capability state after transfer", () => {
    const { invalidateQueries, removeQueries } = dispatch({
      type: "workspace.ownership_transferred",
      workspaceId: "workspace-1",
      fromUserId: "user-1",
      toUserId: "user-2",
      transferId: "transfer-1",
    });
    expect(keyHeads(removeQueries)).toEqual(
      expect.arrayContaining([
        "workspace-file",
        "file-tree",
        "file-content",
        "workspace-preview",
      ]),
    );
    expect(keyHeads(invalidateQueries)).toEqual(
      expect.arrayContaining([
        "workspace",
        "workspace-vm",
        "docs",
        "workspace-changes",
        "chat-sessions",
      ]),
    );
  });

  it("invalidates live file projections when a watched VM path changes", () => {
    const { invalidateQueries } = dispatch({
      type: "workspace.file_changed",
      workspaceId: "workspace-1",
      repoId: "repo-1",
      path: "src/main.rs",
      rescanRequired: false,
    });
    expect(keyHeads(invalidateQueries)).toEqual(
      expect.arrayContaining([
        "workspace-file",
        "workspace-changes",
        "file-tree",
        "file-content",
      ]),
    );
  });
});

it("reads an open file again when it changes during its first read", async () => {
  // TanStack keeps a first read that is already out instead of cancelling
  // it, and its pre-change answer would end the invalidation. The handler
  // cancels such a read first so the next one goes out after the change
  // (`g5.documents.files-view.refresh`).
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const queryKey = ["workspace-file", "workspace-1", "repo-1", "src/main.rs"];
  let resolveFirst: (value: string) => void = () => {};
  const queryFn = vi
    .fn<() => Promise<string>>()
    .mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          resolveFirst = resolve;
        }),
    )
    .mockResolvedValue("after the write");
  const observer = new QueryObserver(queryClient, { queryKey, queryFn });
  const unsubscribe = observer.subscribe(() => {});
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient,
  });
  expect(queryFn).toHaveBeenCalledTimes(1);

  (manager as unknown as { handleEvent: (value: WSEvent) => void }).handleEvent(
    {
      type: "workspace.file_changed",
      workspaceId: "workspace-1",
      repoId: "repo-1",
      path: "src/main.rs",
      rescanRequired: false,
    },
  );
  resolveFirst("before the write");
  await vi.waitFor(() => expect(queryFn).toHaveBeenCalledTimes(2));
  await vi.waitFor(() =>
    expect(queryClient.getQueryData(queryKey)).toBe("after the write"),
  );

  unsubscribe();
  manager.disconnect();
  queryClient.clear();
});

it("reads an open file again at once even while the tree is still being read", async () => {
  // The tree and Changes reads are the ones a refresh waits behind; an
  // open file is cheap and must not wait for them
  // (`g5.documents.files-view.refresh`).
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const treeKey = ["file-tree", "workspace-1", "repo-1", undefined, -1];
  const bodyKey = ["workspace-file", "workspace-1", "repo-1", "src/main.rs"];
  const treeFn = vi.fn(() => new Promise<string[]>(() => {}));
  const bodyFn = vi
    .fn<() => Promise<string>>()
    .mockResolvedValueOnce("before the write")
    .mockResolvedValue("after the write");
  const tree = new QueryObserver(queryClient, {
    queryKey: treeKey,
    queryFn: treeFn,
  });
  const body = new QueryObserver(queryClient, {
    queryKey: bodyKey,
    queryFn: bodyFn,
  });
  const unsubscribeTree = tree.subscribe(() => {});
  const unsubscribeBody = body.subscribe(() => {});
  await vi.waitFor(() =>
    expect(queryClient.getQueryData(bodyKey)).toBe("before the write"),
  );
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient,
  });
  expect(
    queryClient.isFetching({ queryKey: ["file-tree", "workspace-1"] }),
  ).toBe(1);

  (manager as unknown as { handleEvent: (value: WSEvent) => void }).handleEvent(
    {
      type: "workspace.file_changed",
      workspaceId: "workspace-1",
      repoId: "repo-1",
      path: "src/main.rs",
      rescanRequired: false,
    },
  );
  await vi.waitFor(() =>
    expect(queryClient.getQueryData(bodyKey)).toBe("after the write"),
  );
  expect(treeFn).toHaveBeenCalledTimes(1);

  unsubscribeTree();
  unsubscribeBody();
  manager.disconnect();
  queryClient.clear();
});

it("invalidates real cached PR lists when automatic publication completes", async () => {
  const queryClient = new QueryClient();
  const key = ["workspace-pull-requests", "source", "source", "open"];
  const unrelated = ["workspace-pull-requests", "other", "source", "open"];
  queryClient.setQueryData(key, [{ id: "pr-1" }]);
  queryClient.setQueryData(unrelated, []);
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient,
  });
  (manager as unknown as { handleEvent: (value: WSEvent) => void }).handleEvent(
    {
      type: "workspace.pull_request_updated",
      workspaceId: "source",
      pullRequestId: "pr-1",
    },
  );
  await vi.waitFor(() =>
    expect(queryClient.getQueryState(key)?.isInvalidated).toBe(true),
  );
  expect(queryClient.getQueryState(unrelated)?.isInvalidated).toBe(false);
});
