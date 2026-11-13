// =============================================================================
// G5 Client — Publish card button for a local-only branch
// =============================================================================
//
// A local-only branch that is Publish-eligible (its fork parent had this repo
// remote-tracked, `remoteEligible`) offers a labelled **Publish** button on the
// repository card, beside its local-only pill. Repository actions are labelled
// card buttons, never glyphs on the repository chip
// (g5.workspace.remote-ui.actions). The button opens a dialog, like Make
// Remote's, that reuses `BranchAutoPullControl`'s "Publish to Remote" action
// with its real pending and error feedback. On a successful publish the
// dialog closes; the branch flips to remote_tracking, the card renders its
// remote-tracking status instead, and this button unmounts.
//
// SOT:
// - docs/.../auto_pull_publication_options_spec.md §6.2
// - .semlift/specs/software_architecture/g5-api/services/workspace_service D43
// =============================================================================

import { useState } from "react";
import { UploadCloud } from "lucide-react";
import { Modal } from "../ui/UI";
import { BranchAutoPullControl } from "./BranchAutoPullControl";
import { useWorkspaceLiveGate } from "../../lib/workspaceLiveGate";

export interface PublishBranchMenuProps {
  workspaceId: string;
  /** Repo scope for the per-child action. */
  repoId: string;
  /** Display label for the repository (e.g. `org/repo`). */
  repoLabel?: string;
}

/**
 * The repository card's labelled Publish button and its dialog. The shared
 * `Modal` owns Escape, the focus trap and returning focus to the button.
 */
export function PublishBranchMenu({
  workspaceId,
  repoId,
  repoLabel,
}: PublishBranchMenuProps) {
  // Publishing a branch is the owner's and never a review copy's
  // (`g5.workspace.ui.reader-publication`,
  // `g5.governance.review-workspace.exclusions`).
  const gate = useWorkspaceLiveGate();
  const offered = gate
    ? gate.canWrite && !gate.reviewCopy && gate.reviewIdentityKnown
    : true;
  const [open, setOpen] = useState(false);

  if (!offered) return null;

  const close = () => setOpen(false);

  return (
    <span className="inline-flex items-center">
      <button
        type="button"
        data-testid="publish-branch-menu-button"
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Publish this branch to a new remote branch"
        onClick={(event) => {
          event.stopPropagation();
          setOpen(true);
        }}
        className="rounded bg-[var(--g5-text0)] px-2 py-1 text-xs font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
      >
        Publish
      </button>
      {open && (
        <Modal
          onClose={close}
          labelledBy="publish-branch-dialog-title"
          size="sm"
          testId="publish-branch-dialog"
          className="min-w-0 [overflow-wrap:anywhere] p-4 text-sm"
        >
          <div className="flex items-center gap-2">
            <UploadCloud
              size={16}
              aria-hidden="true"
              className="shrink-0 text-[var(--g5-info)]"
            />
            <h2
              id="publish-branch-dialog-title"
              className="min-w-0 text-base font-semibold text-[var(--g5-text0)]"
            >
              Publish branch
            </h2>
          </div>
          {repoLabel && (
            <p className="mt-1 text-xs text-[var(--g5-text2)]">
              Publishing{" "}
              <span className="font-medium text-[var(--g5-text1)]">
                {repoLabel}
              </span>
              .
            </p>
          )}
          <BranchAutoPullControl
            workspaceId={workspaceId}
            repoId={repoId}
            onActionCompleted={close}
            className="mt-3 text-xs"
          />
          <div className="mt-4 flex justify-end">
            <button
              type="button"
              data-testid="publish-branch-cancel"
              onClick={close}
              className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] disabled:opacity-50"
            >
              Cancel
            </button>
          </div>
        </Modal>
      )}
    </span>
  );
}
