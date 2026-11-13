import { describe, expect, it } from "vitest";

import { selectHomeWorkspaces } from "../components/home/HomeWorkspacesTile";
import { buildWorkspaceHierarchy } from "../components/workspace/WorkspaceSwitcher";
import { workspaceMergeTargetsForCurrentWorkspace } from "../pages/WorkspacePage";
import { createMockWorkspace } from "../test/fixtures";
import {
  isPersonalWorkspace,
  withoutPersonalWorkspaces,
  workspaceKind,
} from "./workspaceKind";
import { buildWorkspaceTreeRows } from "./workspaceTree";

const base = createMockWorkspace({
  id: "base",
  name: "base",
  isBase: true,
  ownerUserId: "u1",
});
const fork = createMockWorkspace({
  id: "fork",
  name: "feature",
  clonedFromId: "base",
  ownerUserId: "u1",
});
// A personal workspace has no parent and is never in the org lists
// (personal_workspace g5.personal.home, g5.personal.refusals).
const personal = createMockWorkspace({
  id: "personal",
  name: "users/ada",
  kind: "personal",
  ownerUserId: "u1",
});
// An ordinary workspace whose name looks personal stays ordinary: the
// presentation follows the registry kind, never the name.
const lookalike = createMockWorkspace({
  id: "lookalike",
  name: "users/bob",
  kind: "working",
  clonedFromId: "base",
  ownerUserId: "u1",
});

describe("workspaceKind", () => {
  it("reads the reported kind", () => {
    expect(workspaceKind({ isBase: false, kind: "personal" })).toBe("personal");
    expect(workspaceKind({ isBase: true, kind: "base" })).toBe("base");
    expect(workspaceKind({ isBase: false, kind: "working" })).toBe("working");
  });

  it("derives Base or working from isBase on older servers", () => {
    expect(workspaceKind({ isBase: true })).toBe("base");
    expect(workspaceKind({ isBase: false })).toBe("working");
    expect(workspaceKind({ isBase: false, kind: null })).toBe("working");
  });

  it("never infers personal from a name", () => {
    expect(isPersonalWorkspace(lookalike)).toBe(false);
    expect(isPersonalWorkspace({ ...lookalike, kind: undefined })).toBe(false);
    expect(isPersonalWorkspace(personal)).toBe(true);
    expect(isPersonalWorkspace(null)).toBe(false);
  });

  it("drops only personal workspaces", () => {
    expect(
      withoutPersonalWorkspaces([base, personal, fork, lookalike]).map(
        (workspace) => workspace.id,
      ),
    ).toEqual(["base", "fork", "lookalike"]);
  });
});

describe("personal workspace exclusions", () => {
  const all = [base, personal, fork, lookalike];

  it("leaves personal workspaces out of the lineage tree (fork picker, clone targets)", () => {
    const rows = buildWorkspaceTreeRows(all, { baseFirst: true });
    expect(rows.map((row) => row.workspace.id)).toEqual([
      "base",
      "fork",
      "lookalike",
    ]);
  });

  it("leaves them out of the Home workspaces table, Mine and All", () => {
    for (const filter of ["mine", "all"] as const) {
      expect(
        selectHomeWorkspaces(all, {
          callerUserId: "u1",
          filter,
          sort: "name",
          query: "",
          openedAt: new Map(),
        }).map((workspace) => workspace.id),
      ).not.toContain("personal");
    }
  });

  it("leaves them out of the workspace switcher, even while open", () => {
    const hierarchy = buildWorkspaceHierarchy(all, "personal");
    const ids = [
      ...hierarchy.roots.map((workspace) => workspace.id),
      ...[...hierarchy.childrenByParentId.values()].flatMap((children) =>
        children.map((workspace) => workspace.id),
      ),
    ];
    expect(ids).not.toContain("personal");
    expect(ids).toContain("lookalike");
  });

  it("offers no merge target to or from a personal workspace", () => {
    expect(workspaceMergeTargetsForCurrentWorkspace(personal, all)).toEqual([]);
    expect(
      workspaceMergeTargetsForCurrentWorkspace(
        { id: "child", clonedFromId: "personal" },
        all,
      ),
    ).toEqual([]);
    expect(workspaceMergeTargetsForCurrentWorkspace(fork, all)).toEqual([
      { id: "base", name: "base" },
    ]);
  });
});
