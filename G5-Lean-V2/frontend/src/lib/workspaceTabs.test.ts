import { beforeEach, describe, expect, it } from "vitest";

import {
  WORKSPACE_TABS_CAP,
  addWorkspaceTab,
  moveWorkspaceTab,
  neighbourAfterClose,
  readWorkspaceTabs,
  removeWorkspaceTab,
  renameWorkspaceTab,
  sameWorkspaceTab,
  workspaceTabKey,
  workspaceTabLabel,
  workspaceTabPath,
  workspaceTabsKey,
  writeWorkspaceTabs,
  type WorkspaceTab,
} from "./workspaceTabs";

const a: WorkspaceTab = { orgSlug: "acme", name: "alpha" };
const b: WorkspaceTab = { orgSlug: "acme", name: "beta" };
const c: WorkspaceTab = { orgSlug: "other", name: "gamma" };

// Open workspace tabs (workbench g5.workbench.tabs): per-user list, order is
// strip order, route identity only.
describe("workspaceTabs", () => {
  beforeEach(() => localStorage.clear());

  it("appends a new tab at the end and keeps an open one in place", () => {
    expect(addWorkspaceTab([], a)).toEqual([a]);
    expect(addWorkspaceTab([a, b], c)).toEqual([a, b, c]);
    expect(addWorkspaceTab([a, b], a)).toEqual([a, b]);
  });

  // A rename rewrites the tab in place (g5.workbench.tabs): position kept,
  // a duplicate under the other name dropped, the earlier position winning.
  it("renames a tab in place and merges a duplicate under the other name", () => {
    const alpha2: WorkspaceTab = { orgSlug: "acme", name: "alpha-2" };
    expect(renameWorkspaceTab([a, b, c], a, alpha2)).toEqual([alpha2, b, c]);
    expect(renameWorkspaceTab([a, b, c], b, alpha2)).toEqual([a, alpha2, c]);
    // Both names open: the earlier position is kept, whichever name it held.
    expect(renameWorkspaceTab([a, b, alpha2], a, alpha2)).toEqual([alpha2, b]);
    expect(renameWorkspaceTab([alpha2, b, a], a, alpha2)).toEqual([alpha2, b]);
    // Not open, or no change: the list is unchanged; a case-only rename
    // rewrites the label of the same tab.
    expect(renameWorkspaceTab([b, c], a, alpha2)).toEqual([b, c]);
    expect(renameWorkspaceTab([a, b], a, a)).toEqual([a, b]);
    const upper: WorkspaceTab = { orgSlug: "acme", name: "ALPHA" };
    expect(renameWorkspaceTab([a, b], a, upper)).toEqual([upper, b]);
    // Another organization's tab of the same name is a different tab.
    expect(
      renameWorkspaceTab([c], { orgSlug: "acme", name: "gamma" }, a),
    ).toEqual([c]);
  });

  // A name link to a personal workspace moves its tab onto the kind-resolved
  // route (g5.personal.routes): the route kind is kept, the position too, and
  // a personal tab already open absorbs the name tab (issue #947).
  it("moves a name tab onto its personal route, keeping one tab", () => {
    const named: WorkspaceTab = { orgSlug: "acme", name: "users/alex" };
    const member: WorkspaceTab = {
      orgSlug: "acme",
      name: "users/alex",
      personal: "user",
    };
    const me: WorkspaceTab = { orgSlug: "acme", name: "me", personal: "me" };
    expect(renameWorkspaceTab([a, named, b], named, member)).toEqual([
      a,
      member,
      b,
    ]);
    expect(renameWorkspaceTab([a, named, b], named, me)).toEqual([a, me, b]);
    expect(renameWorkspaceTab([me, a, named], named, me)).toEqual([me, a]);
    expect(renameWorkspaceTab([named, a, member], named, member)).toEqual([
      member,
      a,
    ]);
  });

  // A reorder (g5.workbench.tabs.reorder) moves the stored entry in front
  // of the first shown follower still open in its organization, else behind
  // that organization's last tab; other organizations keep their places.
  it("moves a tab before its first open follower in its organization", () => {
    const d: WorkspaceTab = { orgSlug: "acme", name: "delta" };
    // To the first workspace place, and to the last.
    expect(moveWorkspaceTab([a, b, d], d, [a, b])).toEqual([d, a, b]);
    expect(moveWorkspaceTab([a, b, d], a, [])).toEqual([b, d, a]);
    expect(moveWorkspaceTab([a, b, d], a, [d])).toEqual([b, a, d]);
    // Another organization's tabs keep their places in the stored list.
    expect(moveWorkspaceTab([a, c, b, d], d, [a, b])).toEqual([d, a, c, b]);
    expect(moveWorkspaceTab([a, c, b, d], a, [])).toEqual([c, b, d, a]);
    expect(moveWorkspaceTab([c, a, b], b, [a])).toEqual([c, b, a]);
    // A follower that is not stored (a transient tab, or one closed in
    // another browser tab) is skipped for the next one; with none left the
    // tab goes last in its organization.
    const transient: WorkspaceTab = { orgSlug: "acme", name: "gone" };
    expect(moveWorkspaceTab([a, b, d], a, [transient, d])).toEqual([b, a, d]);
    expect(moveWorkspaceTab([a, c, b], a, [transient])).toEqual([c, b, a]);
    // A follower named another way is the same tab.
    expect(
      moveWorkspaceTab([a, b, d], d, [{ orgSlug: "acme", name: "ALPHA" }]),
    ).toEqual([d, a, b]);
  });

  it("never writes back a tab that is no longer stored", () => {
    // Closed or renamed in another browser tab after the strip showed it: the
    // latest list stays as it is rather than resurrecting the stale tab.
    const transient: WorkspaceTab = { orgSlug: "acme", name: "gone" };
    expect(moveWorkspaceTab([a, b], transient, [a])).toEqual([a, b]);
    expect(moveWorkspaceTab([a, b], c, [a])).toEqual([a, b]);
    // The only tab of its organization stays put.
    expect(moveWorkspaceTab([a, c], c, [])).toEqual([a, c]);
  });

  it("moves the stored entry, keeping a personal route and the stored spelling", () => {
    const me: WorkspaceTab = { orgSlug: "acme", name: "me", personal: "me" };
    const member: WorkspaceTab = {
      orgSlug: "acme",
      name: "users/ann",
      personal: "user",
    };
    // An ordinary workspace whose name looks personal is a different tab.
    const lookalike: WorkspaceTab = { orgSlug: "acme", name: "me" };
    expect(moveWorkspaceTab([a, me, member], me, [a])).toEqual([me, a, member]);
    expect(
      moveWorkspaceTab([a, lookalike, me], { orgSlug: "acme", name: "me" }, [
        a,
      ]),
    ).toEqual([lookalike, a, me]);
    expect(moveWorkspaceTab([a, me, member], member, [me])).toEqual([
      a,
      member,
      me,
    ]);
    // The entry moves as stored, whatever spelling the strip passed.
    const base: WorkspaceTab = { orgSlug: "acme", name: "base" };
    expect(
      moveWorkspaceTab([a, base], { orgSlug: "acme", name: "Base" }, [a]),
    ).toEqual([base, a]);
  });

  it("caps the list by dropping the oldest tabs", () => {
    const many = Array.from({ length: WORKSPACE_TABS_CAP }, (_, i) => ({
      orgSlug: "acme",
      name: `ws-${i}`,
    }));
    const next = addWorkspaceTab(many, c);
    expect(next).toHaveLength(WORKSPACE_TABS_CAP);
    expect(next[0]).toEqual(many[1]);
    expect(next[next.length - 1]).toEqual(c);
  });

  it("removes a tab by identity and tolerates a missing one", () => {
    expect(removeWorkspaceTab([a, b, c], b)).toEqual([a, c]);
    expect(removeWorkspaceTab([a], c)).toEqual([a]);
  });

  it("falls back to the left neighbour only; the first tab goes Home", () => {
    expect(neighbourAfterClose([a, b, c], b)).toEqual(a);
    expect(neighbourAfterClose([a, b, c], c)).toEqual(b);
    expect(neighbourAfterClose([a, b, c], a)).toBeNull();
    expect(neighbourAfterClose([a], a)).toBeNull();
    expect(neighbourAfterClose([a, b], c)).toBeNull();
  });

  it("identifies a tab by its name case-insensitively, as the server does", () => {
    // `/acme/base` (the settings redirects) and `/acme/Base` (a Home row)
    // open the one Base workspace, so they are one tab.
    const routed: WorkspaceTab = { orgSlug: "acme", name: "base" };
    const listed: WorkspaceTab = { orgSlug: "acme", name: "Base" };
    expect(workspaceTabKey(routed)).toBe(workspaceTabKey(listed));
    expect(sameWorkspaceTab(routed, listed)).toBe(true);
    expect(sameWorkspaceTab(routed, { orgSlug: "other", name: "base" })).toBe(
      false,
    );
    expect(addWorkspaceTab([listed], routed)).toEqual([listed]);
    expect(removeWorkspaceTab([a, listed], routed)).toEqual([a]);
    expect(neighbourAfterClose([a, listed], routed)).toEqual(a);
    // The path keeps the name as routed.
    expect(workspaceTabPath(routed)).toBe("/acme/base");
    expect(workspaceTabPath(listed)).toBe("/acme/Base");
  });

  it("routes a tab by its encoded workspace page path", () => {
    expect(workspaceTabPath({ orgSlug: "acme", name: "My Space" })).toBe(
      "/acme/My%20Space",
    );
    expect(workspaceTabPath({ orgSlug: "acme", name: "feat/x" })).toBe(
      "/acme/feat%2Fx",
    );
  });

  it("keeps a personal workspace tab on its kind-resolved route (g5.personal.routes)", () => {
    const me: WorkspaceTab = { orgSlug: "acme", name: "me", personal: "me" };
    const ada: WorkspaceTab = {
      orgSlug: "acme",
      name: "users/ada",
      personal: "user",
    };
    expect(workspaceTabPath(me)).toBe("/acme/me");
    expect(workspaceTabLabel(me)).toBe("Personal workspace");
    expect(workspaceTabPath(ada)).toBe("/acme/users/ada");
    expect(workspaceTabLabel(ada)).toBe("users/ada");
    // An ordinary workspace whose name looks personal keeps its name route.
    const lookalike: WorkspaceTab = { orgSlug: "acme", name: "users/ada" };
    expect(workspaceTabPath(lookalike)).toBe("/acme/users%2Fada");
    expect(sameWorkspaceTab(ada, lookalike)).toBe(false);
    writeWorkspaceTabs("u1", [me, ada]);
    expect(readWorkspaceTabs("u1")).toEqual([me, ada]);
  });

  it("round-trips through per-user storage", () => {
    writeWorkspaceTabs("u1", [a, b]);
    expect(readWorkspaceTabs("u1")).toEqual([a, b]);
    expect(readWorkspaceTabs("u2")).toEqual([]);
    expect(localStorage.getItem(workspaceTabsKey("u1"))).toContain("alpha");
  });

  it("tolerates malformed storage and drops bad or duplicate entries", () => {
    localStorage.setItem(workspaceTabsKey("u1"), "not json");
    expect(readWorkspaceTabs("u1")).toEqual([]);
    localStorage.setItem(
      workspaceTabsKey("u1"),
      JSON.stringify([
        a,
        { orgSlug: "", name: "x" },
        { orgSlug: "acme", name: "x", personal: "other" },
        7,
        null,
        a,
        b,
      ]),
    );
    expect(readWorkspaceTabs("u1")).toEqual([a, b]);
  });
});
