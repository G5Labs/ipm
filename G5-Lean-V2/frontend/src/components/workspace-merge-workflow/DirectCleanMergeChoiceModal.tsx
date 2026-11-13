import type { ReactNode } from "react";
import { AlertTriangle, GitMerge, Power, ShieldCheck, X } from "lucide-react";
import { classNames, Modal } from "../ui/UI";
import type { MergePreflightResponse } from "../../lib/types";
import { useWorkspaceLiveGate } from "../../lib/workspaceLiveGate";
import { StartAndSendNotice } from "../chat/StartAndSendNotice";

/** What the full-workflow (agent) choice asks the server for. */
export interface FullWorkflowOptions {
  /**
   * Start and resolve with agent (g5.git.stopped-rebase.resolve-with-agent):
   * record or join the owner's Start with the resolution brief bound to it.
   */
  startCompute: boolean;
}

export interface DirectCleanMergeChoiceModalProps {
  open: boolean;
  title: string;
  preflight: MergePreflightResponse | null;
  operationKind: "workspace-merge" | "workspace-rebase";
  isSubmitting?: boolean;
  errorMessage?: string | null;
  previewAction?: ReactNode;
  /** Why a Start under way waits for capacity, when the shell knows. */
  startWaitReason?: string | null;
  onCancel: () => void;
  onApprove: () => void;
  onFullWorkflow: (options: FullWorkflowOptions) => void;
}

export function DirectCleanMergeChoiceModal({
  open,
  title,
  preflight,
  operationKind,
  isSubmitting = false,
  errorMessage = null,
  previewAction = null,
  startWaitReason = null,
  onCancel,
  onApprove,
  onFullWorkflow,
}: DirectCleanMergeChoiceModalProps) {
  // Start and resolve with agent (g5.git.stopped-rebase.resolve-with-agent,
  // g5.chat.start-and-send): the owner's stopped or failed VM makes the
  // agent choice the compound action that records Start consent with the
  // resolution brief; a Start already under way is joined and the brief
  // queues behind it. Read before the early return: hooks run every render.
  const liveGate = useWorkspaceLiveGate();
  if (!open || !preflight) return null;

  const isWorkspaceRebase = operationKind === "workspace-rebase";
  const cleanMergeCount = preflight.repoResults.filter(
    (repo) => repo.outcome === "cleanMerge",
  ).length;
  const directlyPublishableCount = preflight.repoResults.filter((repo) =>
    ["cleanMerge", "fastForward", "identical", "sourceAlreadyMerged"].includes(
      repo.outcome,
    ),
  ).length;
  const isFastForwardReview = preflight.decision === "fastForwardOrNoOp";
  const requiresFullWorkflow =
    preflight.decision === "gitConflicts" ||
    preflight.decision === "workflowRequired";
  const showFullWorkflow =
    requiresFullWorkflow || (!isWorkspaceRebase && !isFastForwardReview);
  const showPrimaryAction = !requiresFullWorkflow;
  const workflowRequiredCount = preflight.repoResults.filter((repo) =>
    ["gitConflicts", "workflowRequired"].includes(repo.outcome),
  ).length;
  const agentPath = isWorkspaceRebase && requiresFullWorkflow;
  const startAndResolve = agentPath && (liveGate?.canStart ?? false);
  const startUnderWay =
    agentPath &&
    liveGate != null &&
    liveGate.owner &&
    (liveGate.kind === "starting" || liveGate.kind === "queued");
  const startCompute = startAndResolve || startUnderWay;

  return (
    <Modal
      onClose={onCancel}
      labelledBy="direct-clean-merge-choice-title"
      size="md"
      tier="top"
      closeDisabled={isSubmitting}
      testId="direct-clean-merge-choice-modal"
      className="p-0"
    >
      <div className="flex items-center gap-2 border-b border-[var(--g5-stroke)] px-4 py-3">
        {requiresFullWorkflow ? (
          <AlertTriangle size={17} className="text-[var(--g5-warn)]" />
        ) : (
          <ShieldCheck size={17} className="text-[var(--g5-info)]" />
        )}
        <h2
          id="direct-clean-merge-choice-title"
          className="min-w-0 flex-1 truncate text-sm font-semibold text-[var(--g5-text0)]"
        >
          {title}
        </h2>
        <button
          type="button"
          data-testid="direct-clean-merge-close"
          onClick={onCancel}
          disabled={isSubmitting}
          className="flex h-7 w-7 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:opacity-50"
          title="Close"
        >
          <X size={15} />
        </button>
      </div>

      <div className="space-y-3 px-4 py-4">
        <p className="text-sm text-[var(--g5-text1)]">
          {requiresFullWorkflow
            ? isWorkspaceRebase
              ? "Git cannot complete this update directly. An agent will rebase the current workspace onto the target and resolve conflicts. Review the agent’s result before shipping. The target workspace stays unchanged."
              : "This is not a direct clean merge. Starting now will trigger the full merge workflow."
            : isWorkspaceRebase
              ? "No Git merge conflicts were detected. Review or preview the rebased workspace state, then rebase the current workspace onto the selected target. The target workspace will not be advanced."
              : isFastForwardReview
                ? "No Git merge conflicts were detected. Review or preview the merged workspace state, then approve the merge when it matches what you expect."
                : "No Git merge conflicts were detected. Direct merge can publish this branch now, but it skips the full G5 rebase workflow, risk exploration, and mitigation."}
        </p>

        <div className="rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-2 text-xs text-[var(--g5-text1)]">
          {requiresFullWorkflow ? (
            <>
              {workflowRequiredCount} repo
              {workflowRequiredCount === 1 ? "" : "s"} require attention before
              the workspace can be updated.
            </>
          ) : isWorkspaceRebase ? (
            <>
              {directlyPublishableCount} repo
              {directlyPublishableCount === 1 ? "" : "s"} can be rebased
              directly or are already aligned with the selected target.
            </>
          ) : isFastForwardReview ? (
            <>
              {directlyPublishableCount} repo
              {directlyPublishableCount === 1 ? "" : "s"} can publish by
              fast-forward, are already aligned, or already contain the source.
            </>
          ) : (
            <>
              {cleanMergeCount} repo{cleanMergeCount === 1 ? "" : "s"} need a
              clean merge commit. Other repos are already aligned or
              fast-forward eligible.
            </>
          )}
        </div>

        {/* Start and resolve with agent: what the compound action does while
            the VM is stopped or failed; the Start's progress while one is
            under way (g5.chat.start-and-send). */}
        {startAndResolve ? (
          <div
            role="note"
            data-testid="direct-clean-merge-start-and-resolve-note"
            className="flex items-start gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5 text-xs leading-5 text-[var(--g5-text1)]"
          >
            <Power size={13} className="mt-1 shrink-0 text-[var(--g5-text2)]" />
            <span>
              {liveGate?.kind === "failed"
                ? "The workspace failed to start. "
                : "The workspace is stopped. "}
              <strong className="font-medium text-[var(--g5-text0)]">
                Start and resolve with agent
              </strong>{" "}
              starts it and queues the agent’s first message, sent once it is
              running.
            </span>
          </div>
        ) : null}
        {startUnderWay ? (
          <div className="[&>*]:mb-0">
            <StartAndSendNotice waitReason={startWaitReason} />
          </div>
        ) : null}

        {showFullWorkflow && !requiresFullWorkflow && (
          <div className="flex gap-2 rounded-md border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-2 text-xs text-[var(--g5-text1)]">
            <AlertTriangle
              size={15}
              className="mt-0.5 shrink-0 text-[var(--g5-warn)]"
            />
            <p>
              Choose the full workflow when ontology, code-generation,
              grounding, or product-risk exploration is needed.
            </p>
          </div>
        )}

        {errorMessage && (
          <div className="rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]">
            {errorMessage}
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-[var(--g5-stroke)] px-4 py-3">
        {previewAction}
        <button
          type="button"
          data-testid="direct-clean-merge-cancel"
          onClick={onCancel}
          disabled={isSubmitting}
          className="rounded-md px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
        >
          Cancel
        </button>
        {showFullWorkflow && (
          <button
            type="button"
            onClick={() => onFullWorkflow({ startCompute })}
            disabled={isSubmitting}
            data-testid="direct-clean-merge-full-workflow"
            className="inline-flex items-center gap-1.5 rounded-md border border-[var(--g5-stroke)] px-3 py-1.5 text-sm font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
          >
            {startAndResolve ? <Power size={14} /> : <GitMerge size={14} />}
            {isWorkspaceRebase
              ? startAndResolve
                ? "Start and resolve with agent"
                : "Resolve with Agent"
              : "Run Full Rebase Workflow"}
          </button>
        )}
        {showPrimaryAction && (
          <button
            type="button"
            onClick={onApprove}
            disabled={isSubmitting}
            data-testid={
              isWorkspaceRebase
                ? "workspace-rebase-approve"
                : "direct-clean-merge-direct"
            }
            className={classNames(
              "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium text-white",
              isSubmitting
                ? "cursor-not-allowed bg-[var(--g5-ok)]/50"
                : "bg-[var(--g5-ok)] hover:bg-[var(--g5-ok)]/90",
            )}
          >
            <ShieldCheck size={14} />
            {isWorkspaceRebase
              ? "Rebase Workspace"
              : isFastForwardReview
                ? "Approve & Merge"
                : "Direct Merge"}
          </button>
        )}
      </div>
    </Modal>
  );
}
