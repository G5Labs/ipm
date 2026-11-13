import { describe, expect, it } from "vitest";
import { workspaceRibbonActions } from "./workbenchNavigation";
import { WORKBENCH_ACTIONS } from "./workbenchDefaults";
import type { WorkbenchActions } from "./types";

describe("workspace ribbon navigation", () => {
  it("removes approval destinations in every phase without changing the saved configuration", () => {
    const before = structuredClone(WORKBENCH_ACTIONS);
    const projected = workspaceRibbonActions(WORKBENCH_ACTIONS);
    expect(WORKBENCH_ACTIONS).toEqual(before);
    for (const phase of ["be", "se", "ie"] as const) {
      expect(
        projected[phase]
          .flatMap((group) => group.actions)
          .some((action) => action.target === "approvals"),
      ).toBe(false);
      expect(projected[phase].flatMap((group) => group.actions)).toEqual(
        before[phase]
          .flatMap((group) => group.actions)
          .filter((action) => action.target !== "approvals"),
      );
    }
  });
  it("handles configured menus and view aliases without removing other actions", () => {
    const source: WorkbenchActions = {
      be: [
        {
          name: "Review",
          actions: [
            {
              id: "mixed",
              label: "Inspect",
              kind: "menu",
              items: [
                {
                  id: "approval",
                  label: "Sign off",
                  kind: "open",
                  target: "view",
                  tab: "workspace_approvals",
                },
                {
                  id: "preview",
                  label: "Preview",
                  kind: "open",
                  target: "preview",
                },
              ],
            },
            {
              id: "empty",
              label: "Review",
              kind: "menu",
              items: [
                {
                  id: "request",
                  label: "Request",
                  kind: "open",
                  target: "approvals",
                },
              ],
            },
          ],
        },
      ],
      se: [],
      ie: [],
    };
    const output = workspaceRibbonActions(source);
    expect(output.be[0].actions.map((action) => action.id)).toEqual(["mixed"]);
    expect(output.be[0].actions[0].items?.map((action) => action.id)).toEqual([
      "preview",
    ]);
    expect(source.be[0].actions).toHaveLength(2);
    expect(source.be[0].actions[0].items).toHaveLength(2);
  });
});
