// G5 Client — DeleteProjectDocDialog
// The project-doc delete confirmation. It can outlive the VM, and deleting
// needs the owner handle, so Delete reads the page's live gate at the moment
// of confirmation: disabled with the state's reason and the header's Start
// (`g5.workspace.ui.unavailable`), the confirmation staying open.

import { AlertCircle } from "lucide-react";
import { GatedControl, useLiveGateReason } from "../../lib/workspaceLiveGate";
import { Modal } from "../ui/UI";

export interface DeleteProjectDocDialogProps {
  name: string;
  onCancel: () => void;
  /** Called only while the gate admits the delete; the dialog closes first. */
  onConfirm: () => void;
}

export function DeleteProjectDocDialog({
  name,
  onCancel,
  onConfirm,
}: DeleteProjectDocDialogProps) {
  const blockedReason = useLiveGateReason("delete documents");
  return (
    <Modal
      onClose={onCancel}
      labelledBy="project-doc-delete-title"
      size="sm"
      className="p-0"
    >
      <div className="flex items-center gap-2 border-b border-[var(--g5-stroke)] px-4 py-3">
        <AlertCircle size={17} className="shrink-0 text-[var(--g5-blocked)]" />
        <h2
          id="project-doc-delete-title"
          className="min-w-0 flex-1 text-sm font-semibold text-[var(--g5-text0)]"
        >
          Delete project doc?
        </h2>
      </div>
      <div className="space-y-2 px-4 py-3 text-sm text-[var(--g5-text1)]">
        <p>Delete "{name}"? This cannot be undone.</p>
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-[var(--g5-stroke)] px-4 py-3">
        <button
          type="button"
          data-testid="project-doc-delete-cancel"
          className="rounded-md px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
          onClick={onCancel}
        >
          Cancel
        </button>
        <GatedControl
          verb="delete documents"
          reason={blockedReason}
          testId="project-doc-delete-start-hint"
        >
          <button
            type="button"
            data-testid="project-doc-delete-confirm"
            disabled={blockedReason !== null}
            title={blockedReason ?? undefined}
            className="rounded-md bg-[var(--g5-blocked)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-50"
            onClick={() => {
              if (blockedReason !== null) return;
              onConfirm();
            }}
          >
            Delete
          </button>
        </GatedControl>
      </div>
    </Modal>
  );
}
