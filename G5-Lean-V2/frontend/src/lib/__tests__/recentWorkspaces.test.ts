import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  pushRecentWorkspaceOpen,
  readRecentWorkspaceOpens,
  recentWorkspaceOpensKey,
  recordWorkspaceOpen,
  RECENT_WORKSPACE_OPENS_CAP,
  type RecentWorkspaceOpen,
} from "../recentWorkspaces";

const USER_ID = "user-1";

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("recentWorkspaces MRU (routing-and-auth D24)", () => {
  it("pushRecentWorkspaceOpen prepends, dedupes by id, and caps the list", () => {
    const seed: RecentWorkspaceOpen[] = Array.from(
      { length: RECENT_WORKSPACE_OPENS_CAP },
      (_, i) => ({ id: `ws-${i}`, openedAt: "2026-08-01T00:00:00Z" }),
    );

    // Re-opening an existing id moves it to the front with a fresh timestamp.
    const reopened = pushRecentWorkspaceOpen(
      seed,
      "ws-3",
      "2026-08-11T00:00:00Z",
    );
    expect(reopened[0]).toEqual({
      id: "ws-3",
      openedAt: "2026-08-11T00:00:00Z",
    });
    expect(reopened).toHaveLength(RECENT_WORKSPACE_OPENS_CAP);
    expect(reopened.filter((entry) => entry.id === "ws-3")).toHaveLength(1);

    // A new id at the cap evicts the oldest entry.
    const grown = pushRecentWorkspaceOpen(
      seed,
      "ws-new",
      "2026-08-11T00:00:00Z",
    );
    expect(grown).toHaveLength(RECENT_WORKSPACE_OPENS_CAP);
    expect(grown[0].id).toBe("ws-new");
    expect(
      grown.some(
        (entry) => entry.id === `ws-${RECENT_WORKSPACE_OPENS_CAP - 1}`,
      ),
    ).toBe(false);
  });

  it("readRecentWorkspaceOpens tolerates missing, malformed, and mis-shaped storage", () => {
    expect(readRecentWorkspaceOpens(USER_ID)).toEqual([]);

    localStorage.setItem(recentWorkspaceOpensKey(USER_ID), "not-json{");
    expect(readRecentWorkspaceOpens(USER_ID)).toEqual([]);

    localStorage.setItem(
      recentWorkspaceOpensKey(USER_ID),
      JSON.stringify({ nope: true }),
    );
    expect(readRecentWorkspaceOpens(USER_ID)).toEqual([]);

    // Mis-shaped entries are filtered; valid ones survive.
    localStorage.setItem(
      recentWorkspaceOpensKey(USER_ID),
      JSON.stringify([
        { id: "ws-1", openedAt: "2026-08-11T00:00:00Z" },
        { id: 42 },
        "junk",
        null,
      ]),
    );
    expect(readRecentWorkspaceOpens(USER_ID)).toEqual([
      { id: "ws-1", openedAt: "2026-08-11T00:00:00Z" },
    ]);
  });

  it("recordWorkspaceOpen persists per user and round-trips through read", () => {
    recordWorkspaceOpen(USER_ID, "ws-a", () => "2026-08-11T00:00:00Z");
    recordWorkspaceOpen(USER_ID, "ws-b", () => "2026-08-11T01:00:00Z");
    recordWorkspaceOpen("user-2", "ws-c", () => "2026-08-11T02:00:00Z");

    expect(readRecentWorkspaceOpens(USER_ID)).toEqual([
      { id: "ws-b", openedAt: "2026-08-11T01:00:00Z" },
      { id: "ws-a", openedAt: "2026-08-11T00:00:00Z" },
    ]);
    expect(readRecentWorkspaceOpens("user-2")).toEqual([
      { id: "ws-c", openedAt: "2026-08-11T02:00:00Z" },
    ]);
  });

  it("recordWorkspaceOpen still returns the updated list when the storage write fails", () => {
    recordWorkspaceOpen(USER_ID, "ws-a", () => "2026-08-11T00:00:00Z");
    let storageMethodOwner: object | null = localStorage;
    while (
      storageMethodOwner !== null &&
      !Object.prototype.hasOwnProperty.call(storageMethodOwner, "setItem")
    ) {
      storageMethodOwner = Object.getPrototypeOf(storageMethodOwner);
    }
    expect(storageMethodOwner).not.toBeNull();
    vi.spyOn(storageMethodOwner as Storage, "setItem").mockImplementation(
      () => {
        throw new DOMException("quota exceeded", "QuotaExceededError");
      },
    );

    // The write failure is swallowed and the computed list is still returned.
    expect(
      recordWorkspaceOpen(USER_ID, "ws-b", () => "2026-08-11T01:00:00Z"),
    ).toEqual([
      { id: "ws-b", openedAt: "2026-08-11T01:00:00Z" },
      { id: "ws-a", openedAt: "2026-08-11T00:00:00Z" },
    ]);

    // Storage keeps its pre-failure contents.
    vi.restoreAllMocks();
    expect(readRecentWorkspaceOpens(USER_ID)).toEqual([
      { id: "ws-a", openedAt: "2026-08-11T00:00:00Z" },
    ]);
  });
});
