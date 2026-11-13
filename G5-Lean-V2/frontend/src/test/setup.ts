// =============================================================================
// Vitest Global Setup — MSW Lifecycle & jsdom Polyfills
// =============================================================================
// This file is registered as vitest's `setupFiles` entry in vitest.config.ts.
// It runs before every test file and manages:
// - MSW server lifecycle (start, reset, close)
// - jsdom polyfills (matchMedia, ResizeObserver, IntersectionObserver, URL,
//   Range geometry)
//
// onUnhandledRequest: 'error' is NON-NEGOTIABLE — any fetch() call that
// doesn't match a registered MSW handler throws an error, failing the test.
// (See generation/.semlift/specs/g5/client/tests.semlift.md acceptance
// criteria: "Component tests fail if an unmocked API call is made".)
// =============================================================================

// Guard against "Cannot redefine property: Symbol($$jest-matchers-object)"
// which occurs when @testing-library/jest-dom and Playwright's bundled expect
// both try to define the same global symbol.
try {
  await import("@testing-library/jest-dom/vitest");
} catch {
  // Already defined — safe to ignore
}
import { beforeAll, afterEach, afterAll, vi } from "vitest";
import { cleanup, configure } from "@testing-library/react";
import { server } from "./msw/server";
import {
  resetMockBaseReverification,
  resetMockOrgAdminSettings,
  resetMockOrgMembers,
  resetMockWorkspacePhases,
} from "./msw/handlers";
import { _activeTestClients } from "./render";
import { resetFileDraftsForTests } from "../components/files/fileDrafts";

// The bundled Monaco runtime (`components/editor/monacoRuntime.ts`) loads the
// real editor and its web workers, which jsdom cannot run. Component tests
// mock `@monaco-editor/react` and never need it; the browser suite exercises
// the real bundle.
vi.mock("../components/editor/monacoRuntime", () => ({ monaco: {} }));

// Increase waitFor timeout for CI Docker environments where MSW responses
// take longer to resolve under CPU-constrained containers.
configure({ asyncUtilTimeout: 5_000 });

// ---------------------------------------------------------------------------
// MSW Server Lifecycle
// ---------------------------------------------------------------------------

beforeAll(() => {
  server.listen({ onUnhandledRequest: "error" });
});

function resetVitestTimersAfterTest() {
  // Some component tests intentionally use fake timers. If a test exits before
  // restoring them, later files can inherit a frozen clock and time out only in
  // the full-suite CI lane. Reset timers globally after each test.
  vi.clearAllTimers();
  vi.useRealTimers();
}

afterEach(async () => {
  // 1. Cancel all in-flight React Query fetches so undici doesn't deliver
  //    responses into a destroyed jsdom context.
  for (const qc of _activeTestClients) {
    qc.cancelQueries();
    qc.clear();
  }
  _activeTestClients.length = 0;

  // 2. Unmount rendered components.
  cleanup();

  // 3. Reset MSW handlers and stateful mock stores to default.
  server.resetHandlers();
  resetMockOrgAdminSettings();
  resetMockOrgMembers();
  resetMockWorkspacePhases();
  resetMockBaseReverification();

  // 4. FILES drafts live for the browser tab (module state); a test's
  //    unsaved text must not reopen the next test's file in Edit.
  resetFileDraftsForTests();

  if (
    typeof globalThis.localStorage !== "undefined" &&
    typeof globalThis.localStorage.clear === "function"
  ) {
    globalThis.localStorage.clear();
  }

  globalThis.sessionStorage?.clear();

  resetVitestTimersAfterTest();
});

afterAll(() => {
  server.close();
});

// ---------------------------------------------------------------------------
// Suppress jsdom/undici "invalid onError method" unhandled rejections
// ---------------------------------------------------------------------------
// When jsdom's JSDOMDispatcher pipes a fetch response through undici after
// the DOM has been torn down, undici throws "InvalidArgumentError: invalid
// onError method". This is a known jsdom + Node ≥22 undici compatibility
// issue that does NOT indicate a real test failure — it only means a fetch
// response arrived after cleanup completed. Catching it here prevents
// Vitest from flagging it as an unhandled error.
// ---------------------------------------------------------------------------
{
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const proc = (globalThis as any).process as
    | { on: (event: string, handler: (reason: unknown) => void) => void }
    | undefined;
  proc?.on("unhandledRejection", (reason: unknown) => {
    if (
      reason instanceof Error &&
      reason.name === "InvalidArgumentError" &&
      reason.message.includes("invalid onError method")
    ) {
      // Swallow — harmless jsdom/undici teardown race
      return;
    }
    // Re-throw anything else so real errors still fail tests
    throw reason;
  });
}

// ---------------------------------------------------------------------------
// jsdom Polyfills
// ---------------------------------------------------------------------------

// window.matchMedia mock for responsive layout tests
function createMockStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    clear() {
      data.clear();
    },
    getItem(key: string) {
      return data.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(data.keys())[index] ?? null;
    },
    removeItem(key: string) {
      data.delete(key);
    },
    setItem(key: string, value: string) {
      data.set(key, String(value));
    },
  };
}

{
  const storage =
    typeof window !== "undefined" &&
    typeof window.localStorage?.getItem === "function"
      ? window.localStorage
      : createMockStorage();

  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    writable: true,
    value: storage,
  });

  if (
    typeof window !== "undefined" &&
    typeof window.localStorage?.getItem !== "function"
  ) {
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      writable: true,
      value: storage,
    });
  }
}

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

// window.ResizeObserver mock for resizable panel tests
class MockResizeObserver {
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
}

Object.defineProperty(window, "ResizeObserver", {
  writable: true,
  value: MockResizeObserver,
});

// window.IntersectionObserver mock for lazy-loading tests
class MockIntersectionObserver {
  readonly root: Element | null = null;
  readonly rootMargin: string = "";
  readonly thresholds: ReadonlyArray<number> = [];
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
  takeRecords = vi.fn().mockReturnValue([]);
}

Object.defineProperty(window, "IntersectionObserver", {
  writable: true,
  value: MockIntersectionObserver,
});

// Range geometry, which jsdom omits: ProseMirror measures the selection
// through it to scroll the caret into view (the rich Markdown editor).
if (typeof Range.prototype.getClientRects === "undefined") {
  const empty = {
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    width: 0,
    height: 0,
    toJSON: () => ({}),
  };
  Object.defineProperty(Range.prototype, "getBoundingClientRect", {
    configurable: true,
    value: () => empty,
  });
  Object.defineProperty(Range.prototype, "getClientRects", {
    configurable: true,
    value: () => Object.assign([], { item: () => null }),
  });
}

// URL.createObjectURL / URL.revokeObjectURL stubs
if (typeof URL.createObjectURL === "undefined") {
  Object.defineProperty(URL, "createObjectURL", {
    writable: true,
    value: vi.fn(() => "blob:mock-url"),
  });
}

if (typeof URL.revokeObjectURL === "undefined") {
  Object.defineProperty(URL, "revokeObjectURL", {
    writable: true,
    value: vi.fn(),
  });
}
