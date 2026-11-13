import React from "react";
import {
  AlertCircle,
  ExternalLink,
  GitMerge,
  RefreshCw,
  XCircle,
} from "lucide-react";
import {
  useCloseWorkspacePullRequest,
  useMergeWorkspacePullRequest,
  useRefreshWorkspacePullRequest,
  useWorkspacePullRequests,
} from "../../hooks/useWorkspace";
import type {
  WorkspacePullRequestLocalMergeStatus,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { githubPublications } from "../../lib/types";
import { classNames } from "../ui/UI";
import { MergeOverrideAction } from "./MergeOverrideDialog";
import {
  ownerOrBaseAdmin,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

export interface PullRequestListProps {
  /** Workspace whose incoming PRs (role="target", status="open") are shown. */
  workspaceId: string;
  /** Current Git change count, shown by callers that need it. */
  dirtyCount?: number;
  /** Heading rendered above the table. Defaults to "Pull Requests". */
  heading?: string;
}

const STATUS_CLASS: Record<WorkspacePullRequestLocalMergeStatus, string> = {
  ready: "border-emerald-500/40 text-emerald-600 bg-emerald-500/10",
  conflict: "border-red-500/40 text-red-600 bg-red-500/10",
  stale: "border-amber-500/40 text-amber-600 bg-amber-500/10",
  blocked: "border-slate-400/40 text-slate-600 bg-slate-500/10",
  merged: "border-sky-500/40 text-sky-600 bg-sky-500/10",
  closed: "border-slate-400/40 text-slate-500 bg-slate-500/10",
};

function formatTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

function headCount(value: unknown): number {
  return Array.isArray(value) ? value.length : 0;
}

export function mergeBlockReason(pr: WorkspacePullRequestSummary): string {
  return (
    pr.blockingMessage ||
    pr.preflight.message ||
    `Local merge status is ${pr.localMergeStatus}`
  );
}

/** Shown while a pull request carries no server verdict yet. */
export const MERGE_ADMISSION_UNKNOWN =
  "Merge admission not evaluated yet — refresh the pull request";

/**
 * Why this pull request cannot be merged on approval grounds
 * (g5.governance.approval.requirements), or `null` when the server admitted
 * it. The server verdict is the only source: a summary without one is
 * unknown, never "not refused". Only Integration Engineering requirements
 * count.
 */
export function mergeAdmissionReason(
  pr: Pick<WorkspacePullRequestSummary, "mergeAdmission">,
): string | null {
  const admission = pr.mergeAdmission;
  if (!admission) return MERGE_ADMISSION_UNKNOWN;
  if (admission.admitted) return null;
  return admission.reason ?? "Approval required";
}

function formatStatus(value: string): string {
  return value.replace(/_/g, " ");
}

/**
 * Shared, host-agnostic incoming-PR list + row actions (merge / refresh /
 * close). Rendered by the Base `Pull Requests` center tab (via
 * `PullRequestPanel`) and by the non-Base `Create PR` dialog's
 * "PRs from its children" section. Always scoped to `workspaceId` as the PR
 * target with `role="target"`, `status="open"`. The caller owns outer layout
 * (height, width, background); this component renders only the heading, error
 * banner, and table/empty/loading content so both hosts get identical rows and
 * merge semantics.
 */
export function PullRequestList({
  workspaceId,
  heading = "Pull Requests",
}: PullRequestListProps) {
  // Refresh, close and merge are the target owner's, Base's owner included:
  // the server checks ownership (`g5.workspace.ui.reader-publication`). On
  // Base an organization administrator may act too
  // (`g5.governance.pr.base-admin`). A reader sees the list only.
  const gate = useWorkspaceLiveGate();
  const canAct = gate ? ownerOrBaseAdmin(gate) : true;
  const prs = useWorkspacePullRequests(workspaceId, "target", "open");
  const mergePr = useMergeWorkspacePullRequest(workspaceId);
  const refreshPr = useRefreshWorkspacePullRequest(workspaceId);
  const closePr = useCloseWorkspacePullRequest(workspaceId);
  const mergingIdRef = React.useRef<string | null>(null);
  const [mergingId, setMergingId] = React.useState<string | null>(null);
  const [refreshingId, setRefreshingId] = React.useState<string | null>(null);
  const [closingId, setClosingId] = React.useState<string | null>(null);
  const [inlineError, setInlineError] = React.useState<string | null>(null);

  const handleMerge = async (pullRequestId: string) => {
    if (mergingIdRef.current != null) return;
    setInlineError(null);
    mergingIdRef.current = pullRequestId;
    setMergingId(pullRequestId);
    try {
      await mergePr.mutateAsync({ pullRequestId });
    } catch (error) {
      setInlineError(
        error instanceof Error
          ? error.message
          : "Failed to merge pull request.",
      );
    } finally {
      mergingIdRef.current = null;
      setMergingId(null);
    }
  };

  const handleRefresh = async (pullRequestId: string) => {
    setInlineError(null);
    setRefreshingId(pullRequestId);
    try {
      await refreshPr.mutateAsync({ pullRequestId });
    } catch (error) {
      setInlineError(
        error instanceof Error
          ? error.message
          : "Failed to refresh pull request.",
      );
    } finally {
      setRefreshingId(null);
    }
  };

  const handleClose = async (pullRequestId: string) => {
    setInlineError(null);
    setClosingId(pullRequestId);
    try {
      await closePr.mutateAsync({ pullRequestId });
    } catch (error) {
      setInlineError(
        error instanceof Error
          ? error.message
          : "Failed to close pull request.",
      );
    } finally {
      setClosingId(null);
    }
  };

  if (prs.status === "loading") {
    return (
      <div className="flex items-center justify-center gap-2 py-10 text-sm text-[var(--g5-text2)]">
        <RefreshCw size={16} className="animate-spin" />
        Loading pull requests...
      </div>
    );
  }

  if (prs.status === "error") {
    return (
      <div className="flex items-center justify-center gap-2 px-6 py-10 text-sm text-[var(--g5-blocked)]">
        <AlertCircle size={16} />
        {prs.error.message}
      </div>
    );
  }

  return (
    <div>
      <div className="mb-3 flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
          {heading}
        </h2>
        <div
          data-testid="pull-request-count-label"
          className="text-xs text-[var(--g5-text2)]"
        >
          {prs.data.length} active
        </div>
      </div>

      {inlineError && (
        <div
          data-testid="pull-request-merge-error"
          className="mb-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]"
        >
          {inlineError}
        </div>
      )}

      {prs.data.length === 0 ? (
        <div
          data-testid="pull-request-empty"
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-4 py-6 text-center text-sm text-[var(--g5-text2)]"
        >
          No active pull requests
        </div>
      ) : (
        <div className="overflow-x-auto overflow-hidden rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]">
          <table
            data-testid="pull-request-table"
            className="min-w-full border-collapse text-left text-xs"
          >
            <thead className="bg-[var(--g5-bg1)] text-[var(--g5-text2)]">
              <tr>
                <th className="px-3 py-2 font-medium">Title</th>
                <th className="px-3 py-2 font-medium">Source</th>
                <th className="px-3 py-2 font-medium">Target</th>
                <th className="px-3 py-2 font-medium">Merge</th>
                <th className="px-3 py-2 font-medium">Provider</th>
                <th className="px-3 py-2 font-medium">Heads</th>
                <th className="px-3 py-2 font-medium">Updated</th>
                <th className="px-3 py-2 text-right font-medium">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--g5-stroke)]">
              {prs.data.map((pr) => {
                const admissionReason = mergeAdmissionReason(pr);
                const ready =
                  pr.localMergeStatus === "ready" && admissionReason === null;
                // Keep the row locked for the entire commit-then-merge
                // sequence, including the interval before mergePr starts.
                const isMerging = mergingId === pr.id;
                const isRefreshing =
                  refreshingId === pr.id && refreshPr.isLoading;
                const isClosing = closingId === pr.id && closePr.isLoading;
                const rowBusy = isMerging || isRefreshing || isClosing;
                return (
                  <tr
                    key={pr.id}
                    data-testid={`pull-request-row-${pr.id}`}
                    className="bg-[var(--g5-bg0)]"
                  >
                    <td
                      data-testid={`pull-request-title-${pr.id}`}
                      className="max-w-[280px] truncate px-3 py-2 font-medium text-[var(--g5-text0)]"
                      title={pr.title}
                    >
                      {pr.title}
                    </td>
                    <td className="max-w-[220px] truncate px-3 py-2 text-[var(--g5-text1)]">
                      {pr.sourceWorkspaceName}
                    </td>
                    <td className="max-w-[220px] truncate px-3 py-2 text-[var(--g5-text1)]">
                      {pr.targetWorkspaceName}
                    </td>
                    <td className="px-3 py-2">
                      <span
                        className={classNames(
                          "inline-flex rounded-full border px-2 py-0.5 text-[11px] font-medium",
                          STATUS_CLASS[pr.localMergeStatus],
                        )}
                        title={
                          pr.blockingMessage ??
                          admissionReason ??
                          (ready ? "Ready to merge" : mergeBlockReason(pr))
                        }
                      >
                        {pr.localMergeStatus}
                      </span>
                      {admissionReason && !pr.blockingMessage && (
                        <p
                          className="mt-1 max-w-[280px] text-[var(--g5-warn)]"
                          data-testid={`pull-request-admission-${pr.id}`}
                        >
                          {admissionReason}
                        </p>
                      )}
                      {pr.blockingMessage && (
                        <p
                          className="mt-1 max-w-[280px] text-[var(--g5-blocked)]"
                          data-testid={`pull-request-error-${pr.id}`}
                        >
                          {pr.blockingMessage}
                        </p>
                      )}
                    </td>
                    <td className="max-w-[260px] px-3 py-2 text-[var(--g5-text2)]">
                      {githubPublications(pr).length ? (
                        <div className="flex flex-col gap-1">
                          {githubPublications(pr).map((repo) => (
                            <div
                              key={repo.id}
                              className="flex min-w-0 items-center gap-1.5"
                              title={
                                repo.lastError ??
                                `${repo.providerRepoFullName}: ${formatStatus(repo.providerStatus)}`
                              }
                            >
                              {repo.providerPrUrl ? (
                                <a
                                  href={repo.providerPrUrl}
                                  target="_blank"
                                  rel="noreferrer"
                                  className="inline-flex min-w-0 items-center gap-1 text-[var(--g5-info)] hover:underline"
                                >
                                  <span className="truncate">
                                    {repo.providerRepoFullName}
                                    {repo.providerPrNumber
                                      ? ` #${repo.providerPrNumber}`
                                      : ""}
                                  </span>
                                  <ExternalLink size={11} />
                                </a>
                              ) : (
                                <span className="truncate">
                                  {repo.providerRepoFullName}
                                </span>
                              )}
                              <span className="shrink-0 text-[11px] text-[var(--g5-text2)]">
                                {formatStatus(repo.providerStatus)}
                              </span>
                              {repo.mergedOutsideG5 ? (
                                <span
                                  data-testid={`pull-request-merged-outside-${repo.id}`}
                                  className="shrink-0 text-[11px] text-[var(--g5-warn)]"
                                >
                                  merged on GitHub outside G5
                                </span>
                              ) : null}
                              <span className="min-w-0 shrink text-[10px] text-[var(--g5-text2)]">
                                {repo.sourceRemoteBranch}
                                {" -> "}
                                {repo.targetRemoteBranch}
                              </span>
                            </div>
                          ))}
                        </div>
                      ) : (
                        formatStatus(pr.providerStatus)
                      )}
                    </td>
                    <td className="px-3 py-2 text-[var(--g5-text2)]">
                      {headCount(pr.sourceHeads)} repo
                      {headCount(pr.sourceHeads) === 1 ? "" : "s"}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-[var(--g5-text2)]">
                      {formatTime(pr.updatedAt)}
                    </td>
                    <td className="px-3 py-2 text-right">
                      {canAct && (
                        <div className="inline-flex items-center justify-end gap-1">
                          <button
                            type="button"
                            data-testid={`pull-request-refresh-button-${pr.id}`}
                            aria-label="Refresh pull request"
                            onClick={() => void handleRefresh(pr.id)}
                            disabled={rowBusy}
                            title="Refresh local preflight"
                            className={classNames(
                              "inline-flex h-7 w-7 items-center justify-center rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                              "text-[var(--g5-text1)] transition-colors hover:bg-[var(--g5-bg1)] disabled:cursor-not-allowed disabled:opacity-50",
                            )}
                          >
                            <RefreshCw
                              size={13}
                              className={isRefreshing ? "animate-spin" : ""}
                            />
                          </button>
                          <button
                            type="button"
                            data-testid={`pull-request-close-button-${pr.id}`}
                            aria-label="Close pull request"
                            onClick={() => void handleClose(pr.id)}
                            disabled={rowBusy}
                            title="Close pull request"
                            className={classNames(
                              "inline-flex h-7 w-7 items-center justify-center rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                              "text-[var(--g5-text2)] transition-colors hover:bg-[var(--g5-bg1)] disabled:cursor-not-allowed disabled:opacity-50",
                            )}
                          >
                            <XCircle size={13} />
                          </button>
                          <button
                            type="button"
                            data-testid={`pull-request-merge-button-${pr.id}`}
                            onClick={() => void handleMerge(pr.id)}
                            disabled={!ready || rowBusy}
                            title={
                              ready
                                ? "Merge into target"
                                : (admissionReason ?? mergeBlockReason(pr))
                            }
                            className={classNames(
                              "inline-flex items-center gap-1 rounded-[var(--g5-radius-sm)] border px-2 py-1",
                              "text-xs font-medium transition-colors",
                              ready
                                ? "border-[var(--g5-stroke)] text-[var(--g5-text0)] hover:bg-[var(--g5-bg1)]"
                                : "cursor-not-allowed border-[var(--g5-stroke)] text-[var(--g5-text2)] opacity-50",
                            )}
                          >
                            <GitMerge size={12} />
                            {isMerging ? "Merging..." : "Merge"}
                          </button>
                          {/* Beside Merge, never replacing it: offered by
                              the server's preview while the merge is
                              blocked (g5.workbench.ship-to-parent). */}
                          <MergeOverrideAction
                            targetWorkspaceId={workspaceId}
                            targetWorkspaceName={pr.targetWorkspaceName}
                            pullRequestId={pr.id}
                            pullRequestTitle={pr.title}
                            enabled={pr.status === "open" && !ready}
                            disabled={rowBusy}
                            testIdPrefix={`pull-request-${pr.id}`}
                            buttonClassName="rounded-[var(--g5-radius-sm)] px-2 py-1 text-xs"
                            hintClassName="max-w-[180px] text-left text-[10.5px]"
                            onMerged={() => setInlineError(null)}
                          />
                        </div>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
