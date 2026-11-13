// =============================================================================
// G5 Client — Workspace lineage row
// =============================================================================
// One row of a flattened lineage tree (`lib/workspaceTree.ts`): indented by
// depth with a connector to its parent, the workspace glyph, the name (Base
// in bold) and trailing actions. Shared by the organization fork picker and
// Home's workspace directory (workbench g5.workbench.tabs, .shell-chrome).
// =============================================================================

import { Layers } from "lucide-react";
import React from "react";
import { Link } from "react-router-dom";
import type { WorkspaceSummary } from "../../lib/types";
import { classNames } from "../ui/UI";

export interface WorkspaceLineageRowProps {
  workspace: WorkspaceSummary;
  depth: number;
  testId: string;
  /** Route the name links to; omitted renders the plain name. */
  to?: string;
  linkTestId?: string;
  /**
   * Plain-click handler for the name link, replacing the link's own
   * navigation; a modified or middle click keeps the browser's job.
   */
  onOpen?: () => void;
  /** Badges and actions after the name. */
  children?: React.ReactNode;
  className?: string;
}

function isPlainClick(event: React.MouseEvent): boolean {
  return (
    event.button === 0 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey
  );
}

export const WorkspaceLineageRow: React.FC<WorkspaceLineageRowProps> = ({
  workspace,
  depth,
  testId,
  to,
  linkTestId,
  onOpen,
  children,
  className,
}) => {
  const nameClassName = classNames(
    "min-w-0 flex-1 truncate text-sm text-[var(--g5-text0)]",
    workspace.isBase && "font-semibold",
  );
  return (
    <div
      data-testid={testId}
      data-workspace-name={workspace.name}
      data-base={workspace.isBase ? "true" : "false"}
      className={classNames(
        "relative flex items-center gap-2.5 rounded-[var(--g5-radius-sm)] px-1 py-1.5 hover:bg-[var(--g5-bg2)]",
        className,
      )}
      style={depth > 0 ? { marginLeft: `${depth * 22}px` } : undefined}
    >
      {depth > 0 && (
        <span
          aria-hidden
          className="absolute -left-3 top-0 h-1/2 w-2.5 rounded-bl border-b border-l border-[var(--g5-stroke)]"
        />
      )}
      <Layers size={13} className="shrink-0 text-[var(--g5-text2)]" />
      {to ? (
        <Link
          to={to}
          data-testid={linkTestId}
          className={classNames(nameClassName, "hover:underline")}
          onClick={
            onOpen
              ? (event) => {
                  if (!isPlainClick(event)) return;
                  event.preventDefault();
                  onOpen();
                }
              : undefined
          }
        >
          {workspace.name}
        </Link>
      ) : (
        <span className={nameClassName}>{workspace.name}</span>
      )}
      {children}
    </div>
  );
};
