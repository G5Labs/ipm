import React, { useState } from "react";
import { Loader2, Pencil, X } from "lucide-react";
import { classNames, Modal } from "../ui/UI";

export interface RenameWorkspaceDialogProps {
  /** The workspace to rename: a list row or a loaded detail. */
  workspace: { id: string; name: string };
  isRenaming?: boolean;
  errorMessage?: string | null;
  onClose: () => void;
  onConfirm: (name: string) => Promise<void> | void;
}

/**
 * Rename a workspace (g5.workspace.rename). The Git branch keeps its name and
 * links using the old name keep working (g5.workspace.former-names); the
 * server reports validation and conflicts through `errorMessage`.
 */
export function RenameWorkspaceDialog({
  workspace,
  isRenaming = false,
  errorMessage = null,
  onClose,
  onConfirm,
}: RenameWorkspaceDialogProps) {
  const [draft, setDraft] = useState({
    workspaceId: workspace.id,
    name: workspace.name,
  });
  const name = draft.workspaceId === workspace.id ? draft.name : workspace.name;
  const setName = (next: string) =>
    setDraft({ workspaceId: workspace.id, name: next });
  const trimmed = name.trim();
  const canConfirm =
    trimmed.length > 0 && trimmed !== workspace.name && !isRenaming;

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!canConfirm) return;
    try {
      await onConfirm(trimmed);
    } catch {
      // A refused rename is reported through `errorMessage`; the dialog stays
      // open for a correction.
    }
  };

  return (
    <Modal
      labelledBy="rename-workspace-dialog-title"
      onClose={onClose}
      closeDisabled={isRenaming}
      size="sm"
      className="p-4 text-sm"
    >
      <form onSubmit={submit} data-testid="rename-workspace-dialog">
        <div className="flex items-start justify-between gap-3">
          <div className="flex items-center gap-2">
            <Pencil size={16} className="shrink-0 text-[var(--g5-text1)]" />
            <h2
              id="rename-workspace-dialog-title"
              className="text-base font-semibold text-[var(--g5-text0)]"
            >
              Rename {workspace.name}
            </h2>
          </div>
          <button
            type="button"
            data-testid="rename-workspace-close"
            aria-label="Close"
            disabled={isRenaming}
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
          The Git branch keeps its current name. Links that use the old name
          keep working, and the old name stays reserved for this workspace.
        </p>

        <label className="mt-4 block text-xs font-medium text-[var(--g5-text2)]">
          New name
          <input
            autoFocus
            data-testid="rename-workspace-input"
            value={name}
            disabled={isRenaming}
            onChange={(event) => setName(event.target.value)}
            placeholder="feature/exact-name"
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
            data-testid="rename-workspace-error"
            className="mt-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]"
          >
            {errorMessage}
          </div>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            data-testid="rename-workspace-cancel"
            onClick={onClose}
            disabled={isRenaming}
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="submit"
            data-testid="rename-workspace-confirm"
            disabled={!canConfirm}
            className="inline-flex items-center gap-1.5 rounded-[var(--g5-radius-sm)] bg-[var(--g5-info)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-95 disabled:opacity-50"
          >
            {isRenaming && <Loader2 size={13} className="animate-spin" />}
            Rename workspace
          </button>
        </div>
      </form>
    </Modal>
  );
}
