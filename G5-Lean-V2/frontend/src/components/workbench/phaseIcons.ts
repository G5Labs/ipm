// =============================================================================
// G5 Workbench — phase icons
// =============================================================================
// The one icon per engineering phase (workbench g5.workbench.phases), shared
// by the workspace chevrons and the Home stepper so both surfaces tell the
// same story: briefcase (business), blocks (system), plug (integration).
// =============================================================================

import type React from "react";
import { Blocks, Briefcase, Plug } from "lucide-react";
import type { WorkspacePhase } from "../../lib/types";

export type PhaseIcon = React.ComponentType<{
  size?: number;
  className?: string;
  style?: React.CSSProperties;
}>;

export const PHASE_ICONS: Record<WorkspacePhase, PhaseIcon> = {
  be: Briefcase,
  se: Blocks,
  ie: Plug,
};
