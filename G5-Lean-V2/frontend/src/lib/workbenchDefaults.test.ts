import { describe, expect, it } from "vitest";

import type { WorkbenchAction } from "./types";
import { WORKBENCH_ACTIONS, WORKSPACE_PHASES } from "./workbenchDefaults";

// The client fallback is generated from the backend built-ins
// (`workbenchBuiltins.generated.ts`, drift-checked by the Rust test
// `frontend_fallback_matches_the_builtin_actions`). This side checks it is a
// well-formed action set the ribbon can render without the route.
function walk(
  actions: WorkbenchAction[],
  visit: (action: WorkbenchAction) => void,
) {
  for (const action of actions) {
    visit(action);
    if (action.items) walk(action.items, visit);
  }
}

describe("WORKBENCH_ACTIONS (client fallback)", () => {
  it("covers every phase with the V1 group names", () => {
    expect(WORKBENCH_ACTIONS.be.map((group) => group.name)).toEqual([
      "Author",
      "Analysis",
      "Mock",
      "Alignment",
    ]);
    expect(WORKBENCH_ACTIONS.se.map((group) => group.name)).toEqual([
      "Author",
      "Analysis",
      "Design",
      "Build",
      "Land",
    ]);
    expect(WORKBENCH_ACTIONS.ie.map((group) => group.name)).toEqual([
      "Author",
      "Analysis",
      "Release",
    ]);
  });

  it("holds only well-formed actions with unique ids per phase", () => {
    for (const phase of WORKSPACE_PHASES) {
      const ids = new Set<string>();
      walk(
        WORKBENCH_ACTIONS[phase].flatMap((group) => group.actions),
        (action) => {
          expect(action.id).toMatch(/^[a-z0-9-]+$/);
          expect(ids.has(action.id)).toBe(false);
          ids.add(action.id);
          expect(action.label.length).toBeGreaterThan(0);
          expect(["chat", "open", "menu"]).toContain(action.kind);
          if (action.kind === "chat") expect(action.prompt).toBeTruthy();
          if (action.kind === "open") expect(action.target).toBeTruthy();
          if (action.kind === "menu")
            expect(action.items?.length).toBeGreaterThan(0);
          if (action.target === "view") expect(action.tab).toBeTruthy();
        },
      );
    }
  });
});
