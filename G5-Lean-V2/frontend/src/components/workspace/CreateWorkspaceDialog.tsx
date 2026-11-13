import React from "react";

import { useWorkspaces } from "../../hooks/useWorkspace";
import { Button, Modal } from "../ui/UI";
import { CloneDialog } from "./CloneDialog";
import type { ForkLanding } from "../../lib/workspaceLanding";

export interface CreateWorkspaceDialogProps {
  open: boolean;
  onClose: () => void;
  orgId: string;
  /** How the new workspace lands (ui g5.workspace.ui.fork-landing). */
  onCreated?: (workspaceName: string, landing: ForkLanding) => void;
  /** Issue reference to preselect in the fork dialog. */
  initialIssueRef?: string;
}

/**
 * The Create entry point is a presentation wrapper around the sole fork
 * operation. A working workspace always starts from the organization's base
 * workspace, so Create and Fork share exact-name validation, frozen members,
 * collision checks, recovery, and idempotency.
 */
export const CreateWorkspaceDialog: React.FC<CreateWorkspaceDialogProps> = ({
  open,
  onClose,
  orgId,
  onCreated,
  initialIssueRef,
}) => {
  const workspaces = useWorkspaces(open ? orgId : "");

  if (!open) return null;

  if (workspaces.status === "ok") {
    const base = workspaces.data.find((workspace) => workspace.isBase);
    if (base) {
      return (
        <CloneDialog
          source={base}
          orgId={orgId}
          onClose={onClose}
          onCloned={(workspace, landing) =>
            onCreated?.(workspace.name, landing)
          }
          initialIssueRef={initialIssueRef}
        />
      );
    }
  }

  const loading = workspaces.status === "loading";
  return (
    <Modal
      onClose={onClose}
      labelledBy="create-workspace-dialog-title"
      size="sm"
    >
      <div className="p-6">
        <h2
          id="create-workspace-dialog-title"
          className="text-lg font-semibold text-[var(--g5-text1)]"
        >
          Create workspace
        </h2>
        <p
          className="mt-3 text-sm text-[var(--g5-text2)]"
          role={loading ? "status" : "alert"}
        >
          {loading
            ? "Loading the organization base workspace…"
            : workspaces.status === "error"
              ? "The base workspace could not be loaded. Retry after the workspace list is available."
              : "This organization has no base workspace to fork."}
        </p>
        <div className="mt-6 flex justify-end">
          <Button kind="secondary" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
    </Modal>
  );
};

export default CreateWorkspaceDialog;
