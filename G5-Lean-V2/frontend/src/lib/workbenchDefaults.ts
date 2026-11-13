// =============================================================================
// G5 Workbench — phases and the client fallback for ribbon actions
// =============================================================================
// Data behind the alignment layout (workbench g5.workbench.phases,
// g5.workbench.actions): the three engineering phases with their hues, and
// the shipped V1 ribbon action set. The action set is served by
// `GET /api/v1/workspaces/:id/workbench` (built-ins merged with the
// workspace's committed `.g5/workbench/`); `WORKBENCH_ACTIONS` below is the
// CLIENT FALLBACK for a route that is unavailable (network, 5xx), generated
// from the backend's built-ins (`workbenchBuiltins.generated.ts`, checked by
// `g5-api-core workbench_config::frontend_fallback_matches_the_builtin_actions`).
// The fallback carries no revision, so templated chats cannot start from it;
// `open` actions still work.
// =============================================================================

import type {
  ActionBarStyle,
  PhaseBarStyle,
  WorkbenchActions,
  WorkspacePhase,
} from "./types";
import { WORKBENCH_BUILTIN_ACTIONS } from "./workbenchBuiltins.generated";

/** Phase order as the chevrons show it (g5.workbench.phases). */
export const WORKSPACE_PHASES: readonly WorkspacePhase[] = ["be", "se", "ie"];

/** Type guard for a DTO/stored phase literal. */
export function isWorkspacePhase(value: unknown): value is WorkspacePhase {
  return (
    typeof value === "string" &&
    (WORKSPACE_PHASES as readonly string[]).includes(value)
  );
}

/** Zero-based position of a phase in the chevron order. */
export function phaseIndex(phase: WorkspacePhase): number {
  return WORKSPACE_PHASES.indexOf(phase);
}

/** Phase bar styles (workbench g5.workbench.header-styles), in settings order. */
export const PHASE_BAR_STYLES: readonly PhaseBarStyle[] = [
  "extended",
  "minimal",
  "vertical",
];

export function isPhaseBarStyle(value: unknown): value is PhaseBarStyle {
  return (
    typeof value === "string" &&
    (PHASE_BAR_STYLES as readonly string[]).includes(value)
  );
}

/** Action bar styles (workbench g5.workbench.header-styles), in settings order. */
export const ACTION_BAR_STYLES: readonly ActionBarStyle[] = [
  "extended-icons",
  "minimal-icons",
  "extended-text",
  "simplified",
  "dynamic",
];

export function isActionBarStyle(value: unknown): value is ActionBarStyle {
  return (
    typeof value === "string" &&
    (ACTION_BAR_STYLES as readonly string[]).includes(value)
  );
}

export interface WorkbenchPhaseInfo {
  id: WorkspacePhase;
  /** Two-digit index shown in the chevron. */
  index: string;
  name: string;
  /** Two-letter abbreviation (chat persona pill, compact surfaces). */
  short: string;
  /** What the phase delivers (chevron subline). */
  subline: string;
  /** Design token carrying the phase hue (index.css `:root`). */
  token: string;
  /** Design token carrying the phase's soft surface tint. */
  softToken: string;
}

/**
 * The three phases (g5.workbench.phases). Token names are literal strings so
 * the design-token guard (design_system D6) can check they exist in index.css.
 */
export const WORKBENCH_PHASES: Record<WorkspacePhase, WorkbenchPhaseInfo> = {
  be: {
    id: "be",
    index: "01",
    name: "Business Engineering",
    short: "BE",
    subline: "Requirement doc + mock",
    token: "--g5-phase-be",
    softToken: "--g5-phase-be-soft",
  },
  se: {
    id: "se",
    index: "02",
    name: "System Engineering",
    short: "SE",
    subline: "Design doc + prototype",
    token: "--g5-phase-se",
    softToken: "--g5-phase-se-soft",
  },
  ie: {
    id: "ie",
    index: "03",
    name: "Integration Engineering",
    short: "IE",
    subline: "Code update + operation plan",
    token: "--g5-phase-ie",
    softToken: "--g5-phase-ie-soft",
  },
};

/**
 * Client fallback action set (see the file header): the backend built-ins,
 * generated. Edit the YAML in
 * `backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml` and
 * regenerate with `UPDATE_WORKBENCH_BUILTINS=1 cargo test -p g5-api-core
 * frontend_fallback`; never edit `workbenchBuiltins.generated.ts` by hand.
 */
export const WORKBENCH_ACTIONS: WorkbenchActions = WORKBENCH_BUILTIN_ACTIONS;
