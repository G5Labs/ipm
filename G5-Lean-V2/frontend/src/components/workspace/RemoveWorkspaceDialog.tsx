import React, { useState } from "react";
import { AlertTriangle, Loader2, X } from "lucide-react";
import type { WorkspaceSummary } from "../../lib/types";
import { classNames, Modal } from "../ui/UI";

export interface RemoveWorkspaceDialogProps {
  workspace: WorkspaceSummary;
  isRemoving?: boolean;
  errorMessage?: string | null;
  onClose: () => void;
  onConfirm: () => Promise<void> | void;
}

export function RemoveWorkspaceDialog({
  workspace,
  isRemoving = false,
  errorMessage = null,
  onClose,
  onConfirm,
}: RemoveWorkspaceDialogProps) {
  const [confirmationState, setConfirmationState] = useState({
    workspaceId: workspace.id,
    name: "",
  });
  const confirmationTargetName = workspace.name;
  const confirmationName =
    confirmationState.workspaceId === workspace.id
      ? confirmationState.name
      : "";
  const setConfirmationName = (name: string) =>
    setConfirmationState({ workspaceId: workspace.id, name });
  const canConfirm = confirmationName === confirmationTargetName && !isRemoving;

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!canConfirm) return;
    try {
      await onConfirm();
    } catch {
      // A refused removal is reported through `errorMessage`; the dialog
      // stays open for a retry, so the rejection has nothing left to say.
    }
  };

  return (
    <Modal
      labelledBy="remove-workspace-dialog-title"
      onClose={onClose}
      closeDisabled={isRemoving}
      size="sm"
      className="p-4 text-sm"
    >
      <form onSubmit={submit}>
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-center gap-2">
            <AlertTriangle
              size={16}
              className="shrink-0 text-[var(--g5-warn)]"
            />
            <h2
              id="remove-workspace-dialog-title"
              className="text-base font-semibold text-[var(--g5-text0)]"
            >
              Are you sure you want to remove this workspace?
            </h2>
          </div>
          <button
            type="button"
            data-testid="remove-workspace-close"
            aria-label="Close"
            disabled={isRemoving}
            onClick={onClose}
            className={classNames(
              "inline-flex h-7 w-7 shrink-0 items-center justify-center",
              "rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)]",
              "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
              "disabled:cursor-not-allowed disabled:opacity-50",
            )}
          >
            <X size={15} />
          </button>
        </div>

        <p className="mt-3 text-sm text-[var(--g5-text1)]">
          This removes the workspace from your org and reserves its name. It
          won't be listed anymore.
        </p>

        <div className="mt-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2">
          <div className="truncate font-medium text-[var(--g5-text0)]">
            {confirmationTargetName}
          </div>
        </div>

        <label className="mt-4 block text-xs font-medium text-[var(--g5-text2)]">
          Type the workspace name to confirm
          <input
            autoFocus
            value={confirmationName}
            disabled={isRemoving}
            onChange={(event) => setConfirmationName(event.target.value)}
            className={classNames(
              "mt-1 w-full rounded-[var(--g5-radius-sm)]",
              "border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
              "px-2.5 py-2 text-sm text-[var(--g5-text0)]",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
              "disabled:opacity-60",
            )}
          />
        </label>

        {errorMessage && (
          <div
            role="alert"
            className="mt-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]"
          >
            {errorMessage}
          </div>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            data-testid="remove-workspace-cancel"
            onClick={onClose}
            disabled={isRemoving}
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="submit"
            data-testid="remove-workspace-confirm"
            disabled={!canConfirm}
            className="inline-flex items-center gap-1.5 rounded-[var(--g5-radius-sm)] bg-[var(--g5-blocked)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-95 disabled:opacity-50"
          >
            {isRemoving && <Loader2 size={13} className="animate-spin" />}
            Remove workspace
          </button>
        </div>
      </form>
    </Modal>
  );
}
