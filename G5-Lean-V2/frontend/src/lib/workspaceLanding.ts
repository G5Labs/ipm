// =============================================================================
// Workspace landing — where a page lands after creating or opening a chat
// =============================================================================
// `chatLandingQuery` names a chat to open the workspace page on. A fork
// lands on Chat as the center surface (ui g5.workspace.ui.fork-landing): on
// the chat it started, else on a fresh composer carrying the fork's draft.
// A Base re-verification run's fork adds the draft's seed header
// (ontology_and_harness g5.reverification.workspace).
// =============================================================================

import type { ComposerSeedHeader } from "./reviewPrompts";

export function chatLandingQuery(chatSessionId?: string | null): string {
  return chatSessionId ? `?session=${encodeURIComponent(chatSessionId)}` : "";
}

/** How a fork lands: the chat it started, else a fresh composer with `draft`. */
export interface ForkLanding {
  /** The chat the fork started (Start work), to open the workspace on. */
  chatSessionId: string | null;
  /**
   * Unsent text for the fresh composer when no chat started: the issue's
   * suggested first message, or the message a failed chat start did not send.
   */
  draft: string | null;
  /**
   * Shown at the top of the fresh chat while the draft is pending, as a
   * composer seed's header is (ui g5.workbench.review-prompts).
   */
  header?: ComposerSeedHeader;
}

/** Router state the fork landing travels in; read once by the workspace page. */
export interface ForkLandingState {
  forkLanding: { draft: string | null; header?: ComposerSeedHeader };
}

/** The workspace page query for a fork landing. */
export function forkLandingQuery(landing: ForkLanding): string {
  return landing.chatSessionId
    ? chatLandingQuery(landing.chatSessionId)
    : "?view=chat";
}

/** The router state for a fork landing; none when the fork started a chat. */
export function forkLandingState(
  landing: ForkLanding,
): ForkLandingState | undefined {
  if (landing.chatSessionId) return undefined;
  return {
    forkLanding: landing.header
      ? { draft: landing.draft, header: landing.header }
      : { draft: landing.draft },
  };
}

/** A seed header carried by router state, if it is one the composer shows. */
function readSeedHeader(value: unknown): ComposerSeedHeader | null {
  if (!value || typeof value !== "object") return null;
  const header = value as { kind?: unknown; iterations?: unknown };
  if (header.kind === "verification") return { kind: "verification" };
  if (header.kind === "base-verification") return { kind: "base-verification" };
  if (
    header.kind === "review-loop" &&
    typeof header.iterations === "number" &&
    Number.isFinite(header.iterations)
  ) {
    return { kind: "review-loop", iterations: header.iterations };
  }
  return null;
}

/** The fork landing carried by a location's router state, if any. */
export function readForkLandingState(
  state: unknown,
): ForkLandingState["forkLanding"] | null {
  if (!state || typeof state !== "object") return null;
  const landing = (state as Partial<ForkLandingState>).forkLanding;
  if (!landing || typeof landing !== "object") return null;
  const draft = typeof landing.draft === "string" ? landing.draft : null;
  const header = readSeedHeader(landing.header);
  return header ? { draft, header } : { draft };
}
