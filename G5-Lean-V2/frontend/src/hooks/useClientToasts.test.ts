/**
 * useClientToasts — a toast that offers an action stays long enough to take
 * it; a plain one goes after its severity's delay.
 */

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { toastLifetimeMs, useClientToasts } from "./useClientToasts";

describe("useClientToasts", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("clears pending auto-dismiss timers on unmount", () => {
    const { result, unmount } = renderHook(() => useClientToasts());
    act(() => {
      result.current.pushToast("info", "Plain", "gone soon");
      result.current.pushToast("error", "Broken", "stays longer");
    });
    expect(vi.getTimerCount()).toBe(2);
    unmount();
    // No timer survives to set state on the unmounted hook.
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps a toast with an action for 12 s, a plain one for its severity's delay", () => {
    expect(toastLifetimeMs("info", [])).toBe(4_000);
    expect(
      toastLifetimeMs("info", [
        { label: "Open", action: "navigate", target: "/x" },
      ]),
    ).toBe(12_000);
    expect(
      toastLifetimeMs("error", [
        { label: "Open", action: "navigate", target: "/x" },
      ]),
    ).toBe(12_000);

    const { result } = renderHook(() => useClientToasts());
    act(() => {
      result.current.pushToast("info", "Plain", "gone soon");
      result.current.pushToast("info", "Round 3 pushed", "Sent.", [
        {
          label: "Open on W",
          action: "navigate",
          target: "/g5/w?view=reviews",
        },
      ]);
    });
    expect(result.current.toasts.map((t) => t.title)).toEqual([
      "Plain",
      "Round 3 pushed",
    ]);
    act(() => {
      vi.advanceTimersByTime(4_000);
    });
    expect(result.current.toasts.map((t) => t.title)).toEqual([
      "Round 3 pushed",
    ]);
    act(() => {
      vi.advanceTimersByTime(7_999);
    });
    expect(result.current.toasts).toHaveLength(1);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current.toasts).toHaveLength(0);
  });
});
