import type { WorkbenchAction, WorkbenchActions } from "./types";
import { WORKSPACE_PHASES } from "./workbenchDefaults";

/** Approvals has a stable rail destination. Saved configuration stays intact. */
export function workspaceRibbonActions(
  actions: WorkbenchActions,
): WorkbenchActions {
  const project = (items: WorkbenchAction[]): WorkbenchAction[] =>
    items.flatMap((action) => {
      if (
        action.kind === "open" &&
        (action.target === "approvals" ||
          (action.target === "view" && action.tab === "workspace_approvals"))
      )
        return [];
      if (action.kind !== "menu") return [action];
      const children = project(action.items ?? []);
      return children.length ? [{ ...action, items: children }] : [];
    });
  return Object.fromEntries(
    WORKSPACE_PHASES.map((phase) => [
      phase,
      (actions[phase] ?? [])
        .map((group) => ({ ...group, actions: project(group.actions ?? []) }))
        .filter((group) => group.actions.length),
    ]),
  ) as WorkbenchActions;
}
