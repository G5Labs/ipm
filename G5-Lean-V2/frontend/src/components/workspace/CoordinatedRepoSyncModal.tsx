import { X } from "lucide-react";
import { classNames, Modal } from "../ui/UI";
import type { CoordinatedRepoSyncModalState } from "./CoordinatedRepoSyncModalState";

export interface CoordinatedRepoSyncModalProps extends CoordinatedRepoSyncModalState {
  onClose: () => void;
}

export function CoordinatedRepoSyncModal({
  title,
  message,
  repos,
  onClose,
}: CoordinatedRepoSyncModalProps) {
  return (
    <Modal
      onClose={onClose}
      labelledBy="coordinated-sync-modal-title"
      size="md"
      // The shell's token surface is overridden here so the coordinated-sync
      // failure panel keeps its rose-on-white error treatment; `!` is required
      // because both utilities collide with the shell's base border/background.
      className="border-rose-300! bg-white! p-4 text-sm"
      testId="coordinated-sync-modal"
    >
      <>
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2
              id="coordinated-sync-modal-title"
              className="text-base font-semibold text-rose-900"
            >
              {title}
            </h2>
            <p className="mt-1 text-slate-700">{message}</p>
          </div>
          <button
            type="button"
            data-testid="coordinated-sync-modal-close"
            aria-label="Close"
            onClick={onClose}
            className={classNames(
              "inline-flex shrink-0 items-center justify-center rounded-[var(--g5-radius-sm)] p-1",
              "text-slate-500 hover:bg-slate-100 hover:text-slate-800",
              "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
            )}
          >
            <X size={16} />
          </button>
        </div>

        {repos && repos.length > 0 && (
          <div className="mt-3 overflow-hidden rounded-[var(--g5-radius-sm)] border border-slate-200">
            {repos.map((repo) => (
              <div
                key={`${repo.repoId}-${repo.workspaceId ?? repo.branchName}`}
                className="grid grid-cols-[minmax(0,1fr)_auto] gap-3 border-b border-slate-200 px-3 py-2 last:border-b-0"
              >
                <div className="min-w-0">
                  <div className="truncate font-medium text-slate-900">
                    {repo.repoName}
                  </div>
                  <div className="truncate text-xs text-slate-500">
                    {repo.remoteBranch ?? repo.branchName}
                  </div>
                  {repo.error && (
                    <div className="mt-1 text-xs text-rose-700">
                      {repo.error}
                    </div>
                  )}
                </div>
                <span className="self-start rounded-[var(--g5-radius-sm)] bg-rose-50 px-2 py-0.5 text-xs font-medium text-rose-800">
                  {repo.status}
                </span>
              </div>
            ))}
          </div>
        )}
      </>
    </Modal>
  );
}
