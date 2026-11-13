import { act, fireEvent, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { WorkspaceVmView } from "../lib/types";
import {
  observeWorkspaceUserActivity,
  useWorkspaceUserActivity,
} from "./useWorkspaceUserActivity";

vi.mock("../lib/api", () => ({
  api: { get: vi.fn(), post: vi.fn().mockResolvedValue(null) },
}));
const platformTime = "2026-10-23T22:00:00.000Z";
beforeEach(() => {
  vi.mocked(api.get).mockResolvedValue({ serverTime: platformTime });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.clearAllMocks();
});

function inputListener() {
  const add = vi.spyOn(document, "addEventListener");
  return () =>
    add.mock.calls.find(
      ([type]) => type === "pointerdown",
    )![1] as EventListener;
}

describe("workspace owner activity", () => {
  it("ignores mount, focus, visibility, synthetic events and time passing", () => {
    vi.useFakeTimers();
    const report = vi.fn();
    const dispose = observeWorkspaceUserActivity(document, report);
    fireEvent.focus(window);
    fireEvent(document, new Event("visibilitychange"));
    fireEvent.pointerDown(document.body);
    fireEvent.keyDown(document.body, { key: "a" });
    fireEvent.paste(document.body);
    vi.advanceTimersByTime(24 * 60 * 60 * 1000);
    expect(report).not.toHaveBeenCalled();
    dispose();
  });

  it("reports trusted gestures, coalesces bursts, and excludes terminal controls", () => {
    vi.useFakeTimers();
    const getListener = inputListener();
    const report = vi.fn();
    const dispose = observeWorkspaceUserActivity(document, report);
    const event = (type: string, target = document.body) =>
      ({ type, target, isTrusted: true }) as unknown as Event;
    const listener = getListener();
    listener(event("pointerdown"));
    listener(event("keydown"));
    expect(report).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(10_000);
    const terminal = document.createElement("div");
    terminal.dataset.workspaceActivity = "ignore";
    const input = terminal.appendChild(document.createElement("textarea"));
    listener(event("keydown", input));
    expect(report).toHaveBeenCalledTimes(1);
    listener(event("paste"));
    expect(report).toHaveBeenLastCalledWith("paste", performance.now());
    vi.advanceTimersByTime(10_000);
    listener(event("wheel"));
    expect(report).toHaveBeenLastCalledWith("pointer", performance.now());
    dispose();
  });

  it("keeps refetch/reconnect and failed reports passive; pins reports to the observed VM", async () => {
    vi.useFakeTimers();
    const getListener = inputListener();
    const vm = { vmId: "vm-1", epoch: 7, status: "running" } as WorkspaceVmView;
    vi.mocked(api.post).mockRejectedValue(new Error("offline"));
    const { rerender, unmount } = renderHook(
      ({ current }) => useWorkspaceUserActivity("ws", current, true),
      { initialProps: { current: vm } },
    );
    await act(async () =>
      getListener()({
        type: "keydown",
        target: document.body,
        isTrusted: true,
      } as unknown as Event),
    );
    expect(api.post).toHaveBeenCalledWith(
      "/api/v1/workspaces/ws/user-activity",
      {
        vmId: "vm-1",
        epoch: 7,
        reason: "keyboard",
        observedAt: platformTime,
      },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    rerender({ current: { ...vm } });
    fireEvent(window, new Event("online"));
    fireEvent.focus(window);
    act(() => vi.advanceTimersByTime(60 * 60 * 1000));
    expect(api.post).toHaveBeenCalledTimes(1);
    expect(api.get).toHaveBeenCalledTimes(1);
    unmount();
  });

  it.each([-24, 24])(
    "calibrates a clock skewed by %i hours and keeps the original gesture age",
    async (hours) => {
      vi.useFakeTimers();
      vi.setSystemTime(Date.parse(platformTime) + hours * 60 * 60 * 1000);
      const getListener = inputListener();
      let resolve!: (value: { serverTime: string }) => void;
      vi.mocked(api.get).mockImplementationOnce(
        () =>
          new Promise((done) => {
            resolve = done;
          }),
      );
      const { unmount } = renderHook(() =>
        useWorkspaceUserActivity(
          "ws",
          { vmId: "vm", epoch: 1, status: "running" } as WorkspaceVmView,
          true,
        ),
      );
      const gesture = {
        type: "keydown",
        target: document.body,
        isTrusted: true,
      } as unknown as Event;
      act(() => getListener()(gesture));
      expect(api.post).not.toHaveBeenCalled();
      await act(async () => {
        vi.advanceTimersByTime(2_000);
        resolve({ serverTime: platformTime });
      });
      expect(api.post).toHaveBeenLastCalledWith(
        expect.any(String),
        expect.objectContaining({ observedAt: "2026-10-23T21:59:58.000Z" }),
        expect.any(Object),
      );
      // A backwards wall-clock jump cannot freeze the monotonic throttle.
      vi.setSystemTime(0);
      await act(async () => {
        vi.advanceTimersByTime(10_000);
        getListener()(gesture);
      });
      expect(api.post).toHaveBeenCalledTimes(2);
      unmount();
    },
  );

  it.each(["expired", "suspended", "unmounted"])(
    "drops a clock response after the gesture is %s",
    async (mode) => {
      vi.useFakeTimers();
      const getListener = inputListener();
      if (mode === "suspended") vi.spyOn(performance, "now").mockReturnValue(0);
      let resolve!: (value: { serverTime: string }) => void;
      vi.mocked(api.get).mockImplementationOnce(
        () =>
          new Promise((done) => {
            resolve = done;
          }),
      );
      const { unmount } = renderHook(() =>
        useWorkspaceUserActivity(
          "ws",
          { vmId: "vm", epoch: 1, status: "running" } as WorkspaceVmView,
          true,
        ),
      );
      act(() =>
        getListener()({
          type: "paste",
          target: document.body,
          isTrusted: true,
        } as unknown as Event),
      );
      if (mode === "unmounted") unmount();
      await act(async () => {
        if (mode !== "unmounted") vi.advanceTimersByTime(31_000);
        resolve({ serverTime: platformTime });
      });
      expect(api.post).not.toHaveBeenCalled();
      unmount();
    },
  );

  it("does not install a reporter for a reader or stopped VM", () => {
    const add = vi.spyOn(document, "addEventListener");
    const vm = { vmId: "vm", epoch: 1, status: "running" } as WorkspaceVmView;
    const { rerender } = renderHook(
      ({ owner, current }) => useWorkspaceUserActivity("ws", current, owner),
      { initialProps: { owner: false, current: vm } },
    );
    rerender({ owner: true, current: { ...vm, status: "stopped" } });
    expect(
      add.mock.calls.filter(([name]) => name === "pointerdown"),
    ).toHaveLength(0);
  });
});
