// Tests for the UI interaction telemetry module (issue #1552,
// software_architecture/client/telemetry [I2]).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setApiToken } from "./api";
import {
  describeClickTarget,
  flushTelemetry,
  pendingTelemetryCount,
  recordNavigation,
  recordSettingChange,
  resetTelemetryForTests,
  startTelemetry,
  telemetryRoute,
} from "./telemetry";

/**
 * Fake JWT with a base64url-encoded payload, enough for identity decode. The
 * real token spells the active organization `org`, as the API issues it.
 */
function makeJwt(sub: string, org: string): string {
  const payload = btoa(JSON.stringify({ sub, org }))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return `hdr.${payload}.sig`;
}

type FetchMock = ReturnType<typeof vi.fn>;

function stubFetch(): FetchMock {
  const mock = vi.fn().mockResolvedValue(new Response(null, { status: 202 }));
  vi.stubGlobal("fetch", mock);
  return mock;
}

function setVisibility(state: "visible" | "hidden"): void {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event("visibilitychange"));
}

function sentBodies(mock: FetchMock): Array<{
  sessionId: string;
  events: Array<Record<string, unknown>>;
}> {
  return mock.mock.calls.map((call) => JSON.parse(call[1].body as string));
}

describe("describeClickTarget", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("prefers the nearest data-testid and reports ancestor context", () => {
    document.body.innerHTML = `
      <div data-testid="chat-panel">
        <button data-testid="send-button"><span>Send</span></button>
      </div>`;
    const span = document.querySelector("span")!;
    const identity = describeClickTarget(span)!;
    expect(identity.target).toBe("send-button");
    expect(identity.label).toBe("Send");
    expect(identity.detail).toBe("chat-panel");
  });

  it("falls back to id, then tag name, for untagged interactive elements", () => {
    document.body.innerHTML = `<button id="save-now">Save now</button>`;
    const byId = describeClickTarget(document.querySelector("button")!)!;
    expect(byId.target).toBe("save-now");
    expect(byId.label).toBe("Save now");

    document.body.innerHTML = `<button aria-label="Close dialog">×</button>`;
    const byAria = describeClickTarget(document.querySelector("button")!)!;
    expect(byAria.target).toBe("button");
    expect(byAria.label).toBe("Close dialog");
  });

  it("redacts email addresses in the visible-text fallback", () => {
    document.body.innerHTML = `<button>Remove alice@example.com</button>`;
    const identity = describeClickTarget(document.querySelector("button")!)!;
    expect(identity.label).toBe("Remove <email>");
  });

  it("never reads form-field values", () => {
    document.body.innerHTML = `
      <div data-testid="search-bar">
        <input type="checkbox" name="remember" value="super-secret" checked />
      </div>`;
    const identity = describeClickTarget(document.querySelector("input")!)!;
    expect(identity.target).toBe("search-bar");
    expect(JSON.stringify(identity)).not.toContain("super-secret");
  });

  it("returns null for non-interactive noise clicks", () => {
    document.body.innerHTML = `<p><span>plain text</span></p>`;
    expect(describeClickTarget(document.querySelector("span")!)).toBeNull();
  });

  it("truncates long identity strings and strips control characters", () => {
    const noisy = `bad\nlabel ${"x".repeat(300)}`;
    document.body.innerHTML = `<button aria-label="${noisy}">go</button>`;
    const identity = describeClickTarget(document.querySelector("button")!)!;
    expect(identity.label!.length).toBeLessThanOrEqual(80);
    expect(identity.label).not.toContain("\n");
  });
});

describe("telemetryRoute", () => {
  it("keeps only the view query param, as a separate field", () => {
    expect(telemetryRoute("/acme/base", "?view=settings&secret=x")).toEqual({
      path: "/acme/base",
      view: "settings",
    });
    expect(telemetryRoute("/acme/base", "?other=1")).toEqual({
      path: "/acme/base",
    });
  });

  it("strips path-breaking characters from view", () => {
    expect(telemetryRoute("/a", "?view=set/tings?x")).toEqual({
      path: "/a",
      view: "settingsx",
    });
  });

  it("caps view at 40 UTF-8 bytes, not JS chars", () => {
    const route = telemetryRoute("/a", `?view=${"ü".repeat(30)}`); // 60 bytes
    expect(route.view!.length).toBeLessThanOrEqual(20);
    expect(new TextEncoder().encode(route.view!).length).toBeLessThanOrEqual(
      40,
    );
  });
});

describe("telemetry lifecycle", () => {
  let teardown: (() => void) | null = null;
  let fetchMock: FetchMock;

  beforeEach(() => {
    sessionStorage.clear();
    resetTelemetryForTests();
    fetchMock = stubFetch();
    setApiToken("test-jwt");
  });

  afterEach(() => {
    teardown?.();
    teardown = null;
    resetTelemetryForTests();
    setApiToken(null);
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
    setVisibility("visible");
  });

  it("records ordered clicks with monotonic seq that survives a simulated reload", () => {
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    document.querySelector("button")!.click();
    expect(pendingTelemetryCount()).toBe(2);

    // Simulated reload: teardown flushes, module restarts, seq continues.
    teardown();
    resetTelemetryForTests();
    teardown = startTelemetry();
    document.querySelector("button")!.click();

    setVisibility("hidden");
    const events = sentBodies(fetchMock).flatMap((b) => b.events);
    const seqs = events.map((e) => e.seq as number);
    expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
    expect(new Set(seqs).size).toBe(seqs.length);
    expect(Math.max(...seqs)).toBe(3);
  });

  it("emits navigation events with pathname-only path, view field, and from→to detail", () => {
    teardown = startTelemetry();
    recordNavigation("/acme/base");
    recordNavigation("/acme/base"); // dedupe
    recordNavigation("/acme/base", "settings");
    setVisibility("hidden");
    const events = sentBodies(fetchMock).flatMap((b) => b.events);
    expect(events).toHaveLength(2);
    expect(events[0].kind).toBe("navigation");
    expect(events[1].path).toBe("/acme/base");
    expect(events[1].view).toBe("settings");
    expect(events[1].detail).toBe("/acme/base");
  });

  it("flushes at the size threshold without keepalive", () => {
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="spam">x</button>`;
    for (let i = 0; i < 50; i += 1) {
      document.querySelector("button")!.click();
    }
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/v1/telemetry/ui-events");
    expect(init.keepalive).toBe(false);
    expect(init.headers.Authorization).toBe("Bearer test-jwt");
    expect(sentBodies(fetchMock)[0].events).toHaveLength(50);
    expect(pendingTelemetryCount()).toBe(0);
  });

  it("flushes with keepalive when the tab is hidden", () => {
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    setVisibility("hidden");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][1].keepalive).toBe(true);
  });

  it("sends nothing while unauthenticated and keeps the queue", () => {
    setApiToken(null);
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    setVisibility("hidden");
    expect(fetchMock).not.toHaveBeenCalled();
    expect(pendingTelemetryCount()).toBe(1);
  });

  it("teardown removes listeners and double-start is idempotent", () => {
    teardown = startTelemetry();
    const second = startTelemetry(); // StrictMode double-invoke
    second(); // no-op teardown must not kill the active instance
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    expect(pendingTelemetryCount()).toBe(1);

    teardown();
    teardown = null;
    document.querySelector("button")!.click();
    // Teardown flushed the first click; the post-teardown click records nothing.
    expect(pendingTelemetryCount()).toBe(0);
  });

  it("flushTelemetry never throws even when fetch is broken", () => {
    vi.stubGlobal("fetch", () => {
      throw new Error("boom");
    });
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    expect(() => flushTelemetry("threshold")).not.toThrow();
  });

  it("a synchronous keepalive fetch throw does not latch the in-flight flag", () => {
    // codex impl review finding 8.
    const broken = vi.fn(() => {
      throw new Error("boom");
    });
    vi.stubGlobal("fetch", broken);
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    setVisibility("hidden"); // sync throw inside keepalive flush
    expect(broken).toHaveBeenCalledTimes(1);

    setVisibility("visible");
    fetchMock = stubFetch(); // fetch works again
    document.querySelector("button")!.click();
    setVisibility("hidden");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][1].keepalive).toBe(true);
  });

  it("survives throwing sessionStorage operations", () => {
    // codex impl review finding 5 (private mode / quota).
    const throwing = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
      removeItem: () => {},
      clear: () => {},
      key: () => null,
      length: 0,
    };
    vi.stubGlobal("sessionStorage", throwing);
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    expect(() => document.querySelector("button")!.click()).not.toThrow();
    expect(pendingTelemetryCount()).toBe(1);
    expect(() => setVisibility("hidden")).not.toThrow();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const body = sentBodies(fetchMock)[0];
    expect(body.sessionId).toBeTruthy(); // in-memory fallback session id
    expect(body.events[0].seq).toBe(1); // in-memory fallback seq
  });

  it("flushes under the OLD token and rotates the session on org change", () => {
    // codex impl review finding 2: org-A events must never be posted with
    // org-B claims.
    const orgAToken = makeJwt("user-1", "org-a");
    const orgBToken = makeJwt("user-1", "org-b");
    setApiToken(orgAToken);
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    expect(pendingTelemetryCount()).toBe(1);

    setApiToken(orgBToken); // switchOrg path
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe(
      `Bearer ${orgAToken}`,
    );
    const sessionA = sentBodies(fetchMock)[0].sessionId;

    document.querySelector("button")!.click();
    setVisibility("hidden");
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1][1].headers.Authorization).toBe(
      `Bearer ${orgBToken}`,
    );
    expect(sentBodies(fetchMock)[1].sessionId).not.toBe(sessionA);
  });

  it("records a preference change as a setting event carrying both values", () => {
    teardown = startTelemetry();
    window.history.replaceState(null, "", "/acme/base/settings?view=layout");
    recordSettingChange("sidebarDisplayMode", "rail", "expand-on-hover");
    setVisibility("hidden");
    const events = sentBodies(fetchMock).flatMap((b) => b.events);
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      kind: "setting",
      target: "sidebarDisplayMode",
      label: "rail",
      detail: "from:expand-on-hover",
      path: "/acme/base/settings",
      view: "layout",
    });
    window.history.replaceState(null, "", "/");
  });

  it("reports the preference snapshot once per signed-in identity per tab", () => {
    setApiToken(makeJwt("user-1", "org-a"));
    const preferences = vi.fn(() => [
      ["themeName", "editor"] as const,
      ["showTabBar", "false"] as const,
    ]);
    teardown = startTelemetry({ preferences });
    expect(preferences).toHaveBeenCalledTimes(1);
    expect(pendingTelemetryCount()).toBe(2);
    setVisibility("hidden");
    const events = sentBodies(fetchMock).flatMap((b) => b.events);
    expect(events.map((e) => [e.kind, e.target, e.label, e.detail])).toEqual([
      ["setting", "themeName", "editor", "snapshot"],
      ["setting", "showTabBar", "false", "snapshot"],
    ]);

    // Simulated reload under the same identity: no second snapshot.
    teardown();
    resetTelemetryForTests();
    setApiToken(makeJwt("user-1", "org-a"));
    teardown = startTelemetry({ preferences });
    expect(preferences).toHaveBeenCalledTimes(1);
    expect(pendingTelemetryCount()).toBe(0);
  });

  it("reports the snapshot only once an identity signs in, and again for a new one", () => {
    setApiToken(null);
    const preferences = vi.fn(() => [["themeMode", "dark"] as const]);
    teardown = startTelemetry({ preferences });
    // Unauthenticated start: nothing to attribute the snapshot to.
    expect(preferences).not.toHaveBeenCalled();
    expect(pendingTelemetryCount()).toBe(0);

    const orgAToken = makeJwt("user-1", "org-a");
    setApiToken(orgAToken); // sign-in
    expect(preferences).toHaveBeenCalledTimes(1);
    expect(pendingTelemetryCount()).toBe(1);
    setVisibility("hidden");
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][1].headers.Authorization).toBe(
      `Bearer ${orgAToken}`,
    );
    expect(sentBodies(fetchMock)[0].events[0]).toMatchObject({
      kind: "setting",
      target: "themeMode",
      label: "dark",
      detail: "snapshot",
    });

    setVisibility("visible");
    setApiToken(makeJwt("user-1", "org-b")); // organization switch
    expect(preferences).toHaveBeenCalledTimes(2);
    expect(pendingTelemetryCount()).toBe(1);
    setApiToken(`${makeJwt("user-1", "org-b")}refreshed`); // routine refresh
    expect(preferences).toHaveBeenCalledTimes(2);
  });

  it("a throwing preference source never breaks telemetry start", () => {
    setApiToken(makeJwt("user-1", "org-a"));
    expect(() => {
      teardown = startTelemetry({
        preferences: () => {
          throw new Error("corrupt store");
        },
      });
    }).not.toThrow();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    expect(pendingTelemetryCount()).toBe(1);
  });

  it("does not rotate the session on a same-identity token refresh", () => {
    const before = makeJwt("user-1", "org-a");
    // Same identity claims, different signature — a routine refresh mints a
    // new token string for the same sub/org.
    const after = `${makeJwt("user-1", "org-a")}refreshed`;
    setApiToken(before);
    teardown = startTelemetry();
    document.body.innerHTML = `<button data-testid="one">1</button>`;
    document.querySelector("button")!.click();
    setApiToken(after); // routine 50-min refresh
    expect(fetchMock).not.toHaveBeenCalled();
    expect(pendingTelemetryCount()).toBe(1);
  });
});
