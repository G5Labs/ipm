// Prepare to Ship's approvals refresh schedule (g5.workbench.ship-approvals):
// immediately on open and on return, 1 s for the first 10 visible seconds,
// then 2 s while visible and 15 s while hidden; returning restarts the fast
// window; refreshes never overlap; unmount stops them; a transient failure
// keeps the last good data.
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { api, ApiError } from "../lib/api";
import type { WorkspaceApprovalsStateDTO } from "../lib/types";
import {
  shipApprovalsRefreshDelay,
  startShipApprovalsRefresh,
  useShipApprovalsRefresh,
} from "./useShipApprovalsRefresh";

let visibility: DocumentVisibilityState = "visible";

function setVisibility(next: DocumentVisibilityState) {
  visibility = next;
  document.dispatchEvent(new Event("visibilitychange"));
}

beforeEach(() => {
  visibility = "visible";
  vi.spyOn(document, "visibilityState", "get").mockImplementation(
    () => visibility,
  );
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
  vi.setSystemTime(new Date("2026-10-30T10:00:00Z"));
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

/** Advance fake time and let settled refresh promises schedule the next. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** A refresh that settles at once and records when each one started. */
function instantRefresh() {
  const starts: number[] = [];
  const t0 = Date.now();
  const refresh = vi.fn(() => {
    starts.push(Date.now() - t0);
    return Promise.resolve();
  });
  return { refresh, starts };
}

describe("shipApprovalsRefreshDelay", () => {
  it("is 1 s for the first 10 visible seconds, then 2 s, and 15 s hidden", () => {
    expect(shipApprovalsRefreshDelay(true, 0)).toBe(1_000);
    expect(shipApprovalsRefreshDelay(true, 9_999)).toBe(1_000);
    expect(shipApprovalsRefreshDelay(true, 10_000)).toBe(2_000);
    expect(shipApprovalsRefreshDelay(true, 60_000)).toBe(2_000);
    expect(shipApprovalsRefreshDelay(false, 0)).toBe(15_000);
    expect(shipApprovalsRefreshDelay(false, 60_000)).toBe(15_000);
  });
});

describe("startShipApprovalsRefresh", () => {
  it("refreshes at once, every second for 10 s, then every 2 s while visible", async () => {
    const { refresh, starts } = instantRefresh();
    const stop = startShipApprovalsRefresh({ refresh });
    await advance(14_000);
    expect(starts).toEqual([
      0, 1_000, 2_000, 3_000, 4_000, 5_000, 6_000, 7_000, 8_000, 9_000, 10_000,
      12_000, 14_000,
    ]);
    stop();
  });

  it("slows to 15 s while hidden and, on return, refreshes at once and restarts the fast window", async () => {
    const { refresh, starts } = instantRefresh();
    const stop = startShipApprovalsRefresh({ refresh });
    await advance(12_000); // 0..10 every 1 s, then 12
    starts.length = 0;
    setVisibility("hidden");
    await advance(31_000); // hidden from 12 s to 43 s
    expect(starts).toEqual([27_000, 42_000]);
    starts.length = 0;
    setVisibility("visible"); // at 43 s
    await advance(0);
    expect(starts).toEqual([43_000]);
    await advance(12_000);
    expect(starts).toEqual([
      43_000, 44_000, 45_000, 46_000, 47_000, 48_000, 49_000, 50_000, 51_000,
      52_000, 53_000, 55_000,
    ]);
    stop();
  });

  it("refreshes at once on window focus, once when focus and visibility arrive together", async () => {
    const { refresh, starts } = instantRefresh();
    const stop = startShipApprovalsRefresh({ refresh });
    await advance(20_500); // last refresh at 20 s; next due at 22 s
    starts.length = 0;
    window.dispatchEvent(new Event("focus"));
    await advance(0);
    expect(starts).toEqual([20_500]);
    // Back inside the fast window.
    await advance(1_000);
    expect(starts).toEqual([20_500, 21_500]);

    starts.length = 0;
    setVisibility("hidden");
    await advance(1_000);
    setVisibility("visible");
    window.dispatchEvent(new Event("focus"));
    await advance(0);
    expect(starts).toEqual([22_500]);
    stop();
  });

  it("never starts a refresh while one is in flight, and refreshes again after it when the person returned meanwhile", async () => {
    const resolvers: (() => void)[] = [];
    const refresh = vi.fn(
      () => new Promise<void>((resolve) => resolvers.push(resolve)),
    );
    const stop = startShipApprovalsRefresh({ refresh });
    expect(refresh).toHaveBeenCalledTimes(1);
    await advance(5_000);
    window.dispatchEvent(new Event("focus"));
    await advance(5_000);
    expect(refresh).toHaveBeenCalledTimes(1);
    // The slow response settles; the return during it refreshes at once.
    await act(async () => {
      resolvers[0]();
    });
    expect(refresh).toHaveBeenCalledTimes(2);
    await act(async () => {
      resolvers[1]();
    });
    // Then the regular cadence (fast window restarted by the focus at 5 s).
    await advance(1_000);
    expect(refresh).toHaveBeenCalledTimes(3);
    stop();
  });

  it("keeps its schedule through failed refreshes", async () => {
    const refresh = vi.fn(() => Promise.reject(new Error("offline")));
    const stop = startShipApprovalsRefresh({ refresh });
    await advance(3_000);
    expect(refresh).toHaveBeenCalledTimes(4);
    stop();
  });

  it("stops on close: no timers, no listeners", async () => {
    const { refresh } = instantRefresh();
    const stop = startShipApprovalsRefresh({ refresh });
    await advance(0);
    stop();
    await advance(60_000);
    setVisibility("hidden");
    setVisibility("visible");
    window.dispatchEvent(new Event("focus"));
    await advance(60_000);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("schedules nothing after close when an in-flight refresh settles late", async () => {
    let settle: () => void = () => {};
    const refresh = vi.fn(
      () => new Promise<void>((resolve) => (settle = resolve)),
    );
    const stop = startShipApprovalsRefresh({ refresh });
    stop();
    await act(async () => settle());
    await advance(60_000);
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});

function approvalsState(
  state: "approved" | "pending",
): WorkspaceApprovalsStateDTO {
  return {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "ws-target",
      targetWorkspaceName: "Base",
      requirements: [
        {
          definition: { id: "def-1", name: "Architecture" },
          state,
          approvers: [],
          callerCanDecide: false,
          decisionLog: [],
        } as never,
      ],
    },
  };
}

describe("useShipApprovalsRefresh", () => {
  function wrapper(client: QueryClient) {
    return function Wrapper({ children }: { children: ReactNode }) {
      return (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
      );
    };
  }

  // Real timers: React Query and React deliver this observer's updates
  // through timers that fake timers do not flush; the schedule itself is
  // covered with fake timers above.
  it("reads the shared response on the schedule, keeps the last good data on a transient failure, and stops on unmount", async () => {
    vi.useRealTimers();
    const responses: (() => Promise<unknown>)[] = [
      () => Promise.resolve(approvalsState("pending")),
      () =>
        Promise.reject(
          new ApiError({ code: "unavailable", message: "busy", status: 503 }),
        ),
      () => Promise.resolve(approvalsState("approved")),
    ];
    const get = vi
      .spyOn(api, "get")
      .mockImplementation(
        () =>
          (
            responses.shift() ??
            (() => Promise.resolve(approvalsState("approved")))
          )() as never,
      );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { result, unmount } = renderHook(
      () => useShipApprovalsRefresh("ws-source"),
      { wrapper: wrapper(client) },
    );
    await waitFor(() =>
      expect(result.current).toMatchObject({
        status: "ok",
        data: approvalsState("pending"),
        refreshError: null,
      }),
    );
    // One immediate read, shared with the observer's own first read.
    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(
      "/api/v1/workspaces/ws-source/approvals",
      expect.anything(),
    );

    // The failed poll a second later keeps the last good data.
    await waitFor(
      () =>
        expect(result.current).toMatchObject({
          status: "ok",
          data: approvalsState("pending"),
          refreshError: expect.any(ApiError),
        }),
      { timeout: 2_000 },
    );
    expect(get).toHaveBeenCalledTimes(2);

    await waitFor(
      () =>
        expect(result.current).toMatchObject({
          status: "ok",
          data: approvalsState("approved"),
          refreshError: null,
        }),
      { timeout: 2_000 },
    );
    expect(get).toHaveBeenCalledTimes(3);

    unmount();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 1_500));
    });
    expect(get).toHaveBeenCalledTimes(3);
  });
});
