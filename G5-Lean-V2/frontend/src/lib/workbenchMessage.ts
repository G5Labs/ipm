// =============================================================================
// G5 Workbench — the message text of a templated action
// =============================================================================
// Mirrors `workbench_action_message` in
// `backend/g5-api-core/src/services/agents/workbench_config.rs` (workbench
// g5.workbench.actions): the prompt, then the serialized context, then the
// user's typed text — complete requests travel as message text (the
// `generateChatPrompt.ts` rule). The server builds this for a session start
// (and prepends the persona instructions); the browser builds it for an
// APPEND to a session already carrying the persona, where no prefix is added.
// =============================================================================

import type {
  WorkbenchAction,
  WorkbenchStartContext,
  WorkspacePhase,
} from "./types";
import { WORKBENCH_PHASES } from "./workbenchDefaults";

export function workbenchActionMessage(
  action: Pick<WorkbenchAction, "prompt">,
  phase: WorkspacePhase,
  context: WorkbenchStartContext | undefined,
  userText: string,
): string {
  const sections = [(action.prompt ?? "").trim()];
  if (context?.kind === "current_doc") {
    const path = context.docPath?.trim();
    sections.push(
      path
        ? `Context: the current document is \`${path}\`.`
        : "Context: no document is open; ask which one to use.",
    );
  } else if (context?.kind === "phase") {
    sections.push(
      `Context: the workspace is in the ${WORKBENCH_PHASES[phase].name} phase.`,
    );
  }
  const typed = userText.trim();
  if (typed) sections.push(typed);
  return sections.join("\n\n");
}

/** The context a `chat` action asks for, resolved against the page state. */
export function workbenchActionContext(
  action: Pick<WorkbenchAction, "context">,
  currentDocPath: string | null,
): WorkbenchStartContext | undefined {
  switch (action.context) {
    case "current_doc":
      return { kind: "current_doc", docPath: currentDocPath ?? undefined };
    case "phase":
      return { kind: "phase" };
    case "none":
      return { kind: "none" };
    default:
      return undefined;
  }
}
