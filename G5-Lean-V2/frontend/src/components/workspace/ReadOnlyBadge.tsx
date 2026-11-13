// =============================================================================
// G5 Workspace — ReadOnlyBadge
// =============================================================================
// Small, consistent lock indicator shown wherever a workspace is identified
// (top bar, workspace switcher rows, workspace table rows). Communicates the
// `read_only` mutation policy at a glance (M25). Purely presentational.
// =============================================================================

import React from "react";
import { Lock } from "lucide-react";
import { classNames } from "../ui/UI";

export interface ReadOnlyBadgeProps {
  /** Icon-only compact form for dense rows (still exposes an accessible label). */
  compact?: boolean;
  className?: string;
}

/**
 * A lock badge + "Read-only" label. Render only when the workspace is read-only
 * — callers gate on `ws.readOnly === true` so a missing value stays read-write.
 */
export const ReadOnlyBadge: React.FC<ReadOnlyBadgeProps> = ({
  compact = false,
  className,
}) => {
  return (
    <span
      data-testid="workspace-readonly-badge"
      title="Read-only workspace"
      aria-label="Read-only workspace"
      className={classNames(
        "inline-flex items-center gap-1 shrink-0",
        "rounded-[var(--g5-radius-sm)]",
        "border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10",
        "text-[var(--g5-warn)]",
        compact ? "px-1 py-0.5" : "px-1.5 py-0.5",
        "text-[10px] font-semibold uppercase tracking-wide leading-none",
        className,
      )}
    >
      <Lock size={11} aria-hidden="true" />
      {!compact && <span>Read-only</span>}
    </span>
  );
};

export default ReadOnlyBadge;
