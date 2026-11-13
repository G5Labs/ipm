// =============================================================================
// chunk-reload-guard.test.ts — unit contract for shared-components D8a
// (issue G5Labs/G5#1540)
// =============================================================================
// RED-first regression tests for the lazy-chunk load-failure recovery guard.
// Deploys replace content-hashed lazy chunks (mermaid.core-<hash>.js, monaco,
// katex, …); open tabs then reject dynamic imports until the page reloads.
// The guard listens for Vite's `vite:preloadError` and reloads once per guard
// window — only when the suppression mark persists durably (write + read-back
// verified); `isChunkLoadError` classifies chunk-fetch failures across
// browsers.
// =============================================================================

import { describe, expect, it, vi } from "vitest";
import {
  CHUNK_RELOAD_GUARD_KEY,
  installChunkReloadGuard,
  isChunkLoadError,
  registerChunkReloadSafetyCheck,
} from "./chunk-reload-guard";

interface TestStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

function makeStorage(initial: Record<string, string> = {}): TestStorage & {
  map: Map<string, string>;
} {
  const map = new Map(Object.entries(initial));
  return {
    map,
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => {
      map.set(key, value);
    },
  };
}

function makeThrowingStorage(): TestStorage {
  return {
    getItem: () => {
      throw new Error("storage access denied");
    },
    setItem: () => {
      throw new Error("storage access denied");
    },
  };
}

describe("installChunkReloadGuard", () => {
  it("reloads once on vite:preloadError and marks the guard key", () => {
    const reload = vi.fn();
    const storage = makeStorage();
    const target = new EventTarget();

    const uninstall = installChunkReloadGuard({
      reload,
      storage,
      target,
      now: () => 1_000_000,
    });
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).toHaveBeenCalledTimes(1);
    expect(storage.map.get(CHUNK_RELOAD_GUARD_KEY)).toBe(String(1_000_000));
    uninstall();
  });

  it("suppresses a second reload inside the guard window (no reload loop)", () => {
    const reload = vi.fn();
    const storage = makeStorage();
    const target = new EventTarget();
    let now = 1_000_000;

    const uninstall = installChunkReloadGuard({
      reload,
      storage,
      target,
      now: () => now,
    });
    target.dispatchEvent(new Event("vite:preloadError"));
    now += 5_000; // still inside the default 60s window
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).toHaveBeenCalledTimes(1);
    uninstall();
  });

  it("reloads again once the guard window has elapsed", () => {
    const reload = vi.fn();
    const storage = makeStorage();
    const target = new EventTarget();
    let now = 1_000_000;

    const uninstall = installChunkReloadGuard({
      reload,
      storage,
      target,
      now: () => now,
      windowMs: 60_000,
    });
    target.dispatchEvent(new Event("vite:preloadError"));
    now += 60_001;
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).toHaveBeenCalledTimes(2);
    uninstall();
  });

  it("stops reacting after uninstall", () => {
    const reload = vi.fn();
    const storage = makeStorage();
    const target = new EventTarget();

    const uninstall = installChunkReloadGuard({ reload, storage, target });
    uninstall();
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).not.toHaveBeenCalled();
  });

  it("does not auto-reload when storage is unavailable (no durable suppression)", () => {
    // An in-memory flag resets on location.reload(), so without a persisted
    // mark each fresh document would reload again during a real outage — a
    // reload loop. Non-persistable contexts must skip auto-reload and recover
    // through the consumer reload affordance (D8a item 1).
    const reload = vi.fn();
    const target = new EventTarget();

    const uninstall = installChunkReloadGuard({
      reload,
      storage: makeThrowingStorage(),
      target,
    });
    target.dispatchEvent(new Event("vite:preloadError"));
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).not.toHaveBeenCalled();
    uninstall();
  });

  it("does not auto-reload when the guard mark write throws (read works)", () => {
    // getItem succeeding while setItem throws (quota/private-mode variants)
    // must not reload: the mark would never persist, so every post-reload
    // document would reload again — the same loop by another path.
    const reload = vi.fn();
    const map = new Map<string, string>();
    const target = new EventTarget();

    const uninstall = installChunkReloadGuard({
      reload,
      storage: {
        getItem: (key: string) => map.get(key) ?? null,
        setItem: () => {
          throw new Error("quota exceeded");
        },
      },
      target,
    });
    target.dispatchEvent(new Event("vite:preloadError"));
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).not.toHaveBeenCalled();
    uninstall();
  });

  it("does not auto-reload when the guard mark write silently drops", () => {
    const reload = vi.fn();
    const target = new EventTarget();

    const uninstall = installChunkReloadGuard({
      reload,
      storage: {
        getItem: () => null,
        setItem: () => {
          // write silently dropped — read-back must catch this
        },
      },
      target,
    });
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).not.toHaveBeenCalled();
    uninstall();
  });

  it("fails closed for a dirty live document without consuming the clean reload window", () => {
    const reload = vi.fn();
    const storage = makeStorage();
    const target = new EventTarget();
    let reloadSafe = false;
    const unregisterSafetyCheck = registerChunkReloadSafetyCheck(
      () => reloadSafe,
    );
    const uninstall = installChunkReloadGuard({
      reload,
      storage,
      target,
      now: () => 1_000_000,
    });

    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).not.toHaveBeenCalled();
    expect(storage.map.has(CHUNK_RELOAD_GUARD_KEY)).toBe(false);

    reloadSafe = true;
    target.dispatchEvent(new Event("vite:preloadError"));

    expect(reload).toHaveBeenCalledTimes(1);
    expect(storage.map.get(CHUNK_RELOAD_GUARD_KEY)).toBe(String(1_000_000));
    uninstall();
    unregisterSafetyCheck();
  });
});

describe("isChunkLoadError", () => {
  it("matches Chrome dynamic-import failures", () => {
    expect(
      isChunkLoadError(
        new TypeError(
          "Failed to fetch dynamically imported module: https://dev.g5labs.ai/assets/mermaid.core-RpCSaUEy.js",
        ),
      ),
    ).toBe(true);
  });

  it("matches Firefox dynamic-import failures", () => {
    expect(
      isChunkLoadError(
        new TypeError("error loading dynamically imported module"),
      ),
    ).toBe(true);
  });

  it("matches Safari dynamic-import failures", () => {
    expect(
      isChunkLoadError(new TypeError("Importing a module script failed.")),
    ).toBe(true);
  });

  it("does not match mermaid parse errors", () => {
    expect(
      isChunkLoadError(
        new Error("Parse error on line 2:\ngraph TD;;\n----------^"),
      ),
    ).toBe(false);
  });

  it("does not match non-error values", () => {
    expect(isChunkLoadError(undefined)).toBe(false);
    expect(isChunkLoadError("random string")).toBe(false);
    expect(isChunkLoadError(null)).toBe(false);
  });
});
