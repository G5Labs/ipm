// =============================================================================
// TaskWorkspaceSwitchPrompt — confirm switching workspaces to view a task
// =============================================================================
// A task event can refer to a task in a different workspace from the page that
// received it. This modal names the task's real workspace and offers to switch
// to it, deep-linking straight to the task output.
// =============================================================================

import { taskRoutePath, workspacePagePath } from "../../lib/workspacePagePath";
import React, { useEffect } from "react";
import { AlertCircle } from "lucide-react";

import { useTask } from "../../hooks/useTasks";
import { classNames, Modal } from "../ui/UI";

export interface TaskWorkspaceSwitchPromptProps {
  /** Task the user tried to open workspace changes for. */
  taskId: string;
  /** Workspace currently open in the page. */
  currentWorkspaceId: string;
  /** Org slug, for building the cross-workspace deep link. */
  org: string;
  /** Dismiss the prompt without switching. */
  onClose: () => void;
  /** Navigate to the given in-app path (the task in its real workspace). */
  onSwitch: (path: string) => void;
  /**
   * Called when the task turns out to belong to the *current* workspace —
   * the task list merely missed it before the task cache loaded. Opens
   * its workspace changes in place instead of dropping the user's action.
   */
  onOpenHere: (taskId: string) => void;
}

// ---------------------------------------------------------------------------
// Typed task deep-link
// ---------------------------------------------------------------------------

/**
 * Build an in-app deep link to a task. Falls back to the workspace root when
 * the workspace identity is not known.
 */
export function taskDeepLinkPath(args: {
  org: string;
  workspaceName: string;
  taskId: string;
}): string {
  return (
    workspacePagePath(args.org, args.workspaceName) + taskRoutePath(args.taskId)
  );
}

const BTN_BASE =
  "px-3 py-1.5 rounded-[var(--g5-radius-sm)] text-[12px] font-medium cursor-pointer transition-colors";
const BTN_SECONDARY = classNames(
  BTN_BASE,
  "border border-[var(--g5-stroke)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]",
);
const BTN_PRIMARY = classNames(
  BTN_BASE,
  "bg-[var(--g5-info)] text-white hover:opacity-90",
);

/**
 * The prompt stacks above the workspace dialog the user launched it from, so
 * it renders on the shell's `top` tier.
 */
const PromptDialog: React.FC<{
  children: React.ReactNode;
  labelledBy: string;
  onClose: () => void;
  /** Locating state: neither Escape nor the overlay dismisses it. */
  closeDisabled?: boolean;
}> = ({ children, labelledBy, onClose, closeDisabled }) => (
  <Modal
    onClose={onClose}
    labelledBy={labelledBy}
    size="sm"
    tier="top"
    closeDisabled={closeDisabled}
    testId="task-workspace-switch-prompt"
    className="flex flex-col gap-3 p-4"
  >
    {children}
  </Modal>
);

export const TaskWorkspaceSwitchPrompt: React.FC<
  TaskWorkspaceSwitchPromptProps
> = ({ taskId, currentWorkspaceId, org, onClose, onSwitch, onOpenHere }) => {
  const taskState = useTask(taskId);
  const task = taskState.status === "ok" ? taskState.data : null;

  const isCurrentWorkspace = !!task && task.workspaceId === currentWorkspaceId;

  // Resolve older tasks directly when they are outside the loaded list.
  useEffect(() => {
    if (!isCurrentWorkspace) return;
    onOpenHere(taskId);
  }, [isCurrentWorkspace, taskId, onOpenHere]);

  if (taskState.status === "loading") {
    return (
      <PromptDialog
        onClose={onClose}
        closeDisabled
        labelledBy="task-workspace-switch-locating"
      >
        <div
          id="task-workspace-switch-locating"
          className="flex items-center gap-2 text-sm text-[var(--g5-text1)]"
        >
          <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
          Locating task…
        </div>
      </PromptDialog>
    );
  }

  if (taskState.status === "error" || !task) {
    return (
      <PromptDialog
        onClose={onClose}
        labelledBy="task-workspace-switch-unlocatable"
      >
        <div
          id="task-workspace-switch-unlocatable"
          className="flex items-center gap-2 text-sm text-[var(--g5-text0)]"
        >
          <AlertCircle size={16} className="text-[var(--g5-blocked)]" />
          Could not locate that task&rsquo;s workspace.
        </div>
        <div className="flex justify-end">
          <button
            type="button"
            data-testid="task-workspace-switch-close"
            onClick={onClose}
            className={BTN_SECONDARY}
          >
            Close
          </button>
        </div>
      </PromptDialog>
    );
  }

  if (isCurrentWorkspace) {
    // The effect above opens workspace changes in place (or dismisses when no workspace identity
    // is derivable); render nothing meanwhile.
    return null;
  }

  // Cross-workspace: deep-link straight to the task in its real workspace.
  const targetPath = taskDeepLinkPath({
    org,
    workspaceName: task.workspaceName,
    taskId,
  });

  return (
    <PromptDialog onClose={onClose} labelledBy="task-workspace-switch-title">
      <div
        id="task-workspace-switch-title"
        className="text-sm font-semibold text-[var(--g5-text0)]"
      >
        Open task in another workspace?
      </div>
      <p className="text-[13px] leading-relaxed text-[var(--g5-text1)]">
        This task is running in workspace{" "}
        <span className="font-semibold text-[var(--g5-text0)]">
          {task.workspaceName}
        </span>
        , not the one you have open. Switch to it to view this task?
      </p>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          data-testid="task-workspace-switch-cancel"
          onClick={onClose}
          className={BTN_SECONDARY}
        >
          Cancel
        </button>
        <button
          type="button"
          data-testid="task-workspace-switch-confirm"
          onClick={() => onSwitch(targetPath)}
          className={BTN_PRIMARY}
        >
          Switch workspace
        </button>
      </div>
    </PromptDialog>
  );
};
