import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FileChangeCoalescer } from "./fileChangeCoalescer";
import { invalidateAndReadAgain } from "./readAgain";

const WINDOW_MS = 1000;
const MAX_DEFER_MS = 5000;
const KEEP = { cancelRefetch: false };
const CANCEL = { cancelRefetch: true };

function setup(fetching: () => number = () => 0) {
  const refresh = vi.fn();
  const queryClient = { isFetching: vi.fn(fetching) };
  const coalescer = new FileChangeCoalescer(
    queryClient,
    refresh,
    WINDOW_MS,
    MAX_DEFER_MS,
    () => Date.now(),
  );
  return { refresh, queryClient, coalescer };
}

describe("FileChangeCoalescer", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("refreshes the first change at once when nothing is in flight", () => {
    const { refresh, coalescer } = setup();
    coalescer.changed("ws-1", "repo-a");
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledWith("ws-1", new Set(["repo-a"]), KEEP);
  });

  it("folds a burst into one trailing refresh carrying every repository", () => {
    const { refresh, coalescer } = setup();
    coalescer.changed("ws-1", "repo-a");
    for (let i = 0; i < 200; i += 1) {
      coalescer.changed("ws-1", i % 2 ? "repo-a" : "repo-b");
    }
    expect(refresh).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(refresh).toHaveBeenLastCalledWith(
      "ws-1",
      new Set(["repo-a", "repo-b"]),
      KEEP,
    );
  });

  it("refreshes at most once per window while changes keep arriving", () => {
    const { refresh, coalescer } = setup();
    coalescer.changed("ws-1", "repo-a");
    for (let elapsed = 0; elapsed < WINDOW_MS * 4; elapsed += 100) {
      coalescer.changed("ws-1", "repo-a");
      vi.advanceTimersByTime(100);
    }
    // One leading refresh plus one per completed window.
    expect(refresh).toHaveBeenCalledTimes(1 + 4);
  });

  it("closes a quiet window without refreshing and starts afresh", () => {
    const { refresh, coalescer } = setup();
    coalescer.changed("ws-1", "repo-a");
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(1);
    coalescer.changed("ws-1", "repo-a");
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("keeps workspaces apart", () => {
    const { refresh, coalescer } = setup();
    coalescer.changed("ws-1", "repo-a");
    coalescer.changed("ws-2", "repo-b");
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(refresh).toHaveBeenNthCalledWith(
      2,
      "ws-2",
      new Set(["repo-b"]),
      KEEP,
    );
  });

  it("holds a first change that arrives during a fetch until that fetch has landed", () => {
    let inFlight = 1;
    const { refresh, coalescer, queryClient } = setup(() => inFlight);
    coalescer.changed("ws-1", "repo-a");
    expect(refresh).not.toHaveBeenCalled();
    expect(queryClient.isFetching).toHaveBeenCalledWith({
      queryKey: ["file-tree", "ws-1"],
    });
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).not.toHaveBeenCalled();
    inFlight = 0;
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledWith("ws-1", new Set(["repo-a"]), KEEP);
  });

  it("waits for a fetch in flight at the end of a window instead of cancelling it", () => {
    let inFlight = 0;
    const { refresh, coalescer } = setup(() => inFlight);
    coalescer.changed("ws-1", "repo-a");
    inFlight = 1;
    coalescer.changed("ws-1", "repo-b");
    vi.advanceTimersByTime(WINDOW_MS * 2);
    expect(refresh).toHaveBeenCalledTimes(1);
    inFlight = 0;
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(refresh).toHaveBeenLastCalledWith("ws-1", new Set(["repo-b"]), KEEP);
  });

  it("cancels a fetch that has been in flight too long and reads again", () => {
    const { refresh, coalescer } = setup(() => 1);
    coalescer.changed("ws-1", "repo-a");
    vi.advanceTimersByTime(MAX_DEFER_MS - 1);
    expect(refresh).not.toHaveBeenCalled();
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledWith("ws-1", new Set(["repo-a"]), CANCEL);
  });

  it("never waits behind a fetch when ungated", () => {
    const refresh = vi.fn();
    const queryClient = { isFetching: vi.fn(() => 1) };
    const coalescer = new FileChangeCoalescer(
      queryClient,
      refresh,
      WINDOW_MS,
      MAX_DEFER_MS,
      () => Date.now(),
      false,
    );
    coalescer.changed("ws-1", "repo-a");
    expect(refresh).toHaveBeenCalledTimes(1);
    coalescer.changed("ws-1", "repo-a");
    vi.advanceTimersByTime(WINDOW_MS);
    expect(refresh).toHaveBeenCalledTimes(2);
    expect(queryClient.isFetching).not.toHaveBeenCalled();
  });

  it("drops pending refreshes when cleared", () => {
    const { refresh, coalescer } = setup();
    coalescer.changed("ws-1", "repo-a");
    coalescer.changed("ws-1", "repo-a");
    coalescer.clear();
    vi.advanceTimersByTime(WINDOW_MS * 2);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("makes a real query read again after a change that arrived mid-fetch", async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const queryKey = ["file-tree", "ws-1", "repo-a", undefined, -1];
    let resolveFirst: (value: string[]) => void = () => {};
    const queryFn = vi
      .fn<() => Promise<string[]>>()
      .mockImplementationOnce(
        () =>
          new Promise<string[]>((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockResolvedValue(["after the write"]);
    const observer = new QueryObserver(queryClient, { queryKey, queryFn });
    const unsubscribe = observer.subscribe(() => {});
    const coalescer = new FileChangeCoalescer(
      queryClient,
      (workspaceId, _repoIds, options) =>
        void queryClient.invalidateQueries(
          { queryKey: ["file-tree", workspaceId] },
          options,
        ),
      WINDOW_MS,
      MAX_DEFER_MS,
    );

    // The write lands while the first read is still running.
    expect(queryFn).toHaveBeenCalledTimes(1);
    coalescer.changed("ws-1", "repo-a");
    resolveFirst(["before the write"]);
    await vi.advanceTimersByTimeAsync(0);
    expect(queryClient.getQueryData(queryKey)).toEqual(["before the write"]);

    // Once the stale read has landed, the window's refresh reads again.
    await vi.advanceTimersByTimeAsync(WINDOW_MS);
    expect(queryFn).toHaveBeenCalledTimes(2);
    expect(queryClient.getQueryData(queryKey)).toEqual(["after the write"]);

    unsubscribe();
    coalescer.clear();
    queryClient.clear();
  });

  it("restarts a first read that hangs past the deferral limit", async () => {
    // TanStack's cancelRefetch leaves a first read without data alone, so
    // the forced refresh must cancel it explicitly before invalidating.
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const queryKey = ["file-tree", "ws-1", "repo-a", undefined, -1];
    const queryFn = vi
      .fn<() => Promise<string[]>>()
      .mockImplementationOnce(() => new Promise<string[]>(() => {}))
      .mockResolvedValue(["after the write"]);
    const observer = new QueryObserver(queryClient, { queryKey, queryFn });
    const unsubscribe = observer.subscribe(() => {});
    const coalescer = new FileChangeCoalescer(
      queryClient,
      (workspaceId, _repoIds, options) => {
        const key = ["file-tree", workspaceId];
        if (options.cancelRefetch) {
          void invalidateAndReadAgain(queryClient, key);
        } else {
          void queryClient.invalidateQueries({ queryKey: key }, options);
        }
      },
      WINDOW_MS,
      MAX_DEFER_MS,
    );

    expect(queryFn).toHaveBeenCalledTimes(1);
    coalescer.changed("ws-1", "repo-a");
    // Deferral is measured from the change that found the read in flight.
    await vi.advanceTimersByTimeAsync(MAX_DEFER_MS - 1);
    expect(queryFn).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(WINDOW_MS);
    expect(queryFn).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(0);
    expect(queryClient.getQueryData(queryKey)).toEqual(["after the write"]);

    unsubscribe();
    coalescer.clear();
    queryClient.clear();
  });
});
