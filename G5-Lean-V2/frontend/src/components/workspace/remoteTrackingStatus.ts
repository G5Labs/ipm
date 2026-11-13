// =============================================================================
// G5 — Remote-tracking status derivation (shared)
// =============================================================================
//
// Pure derivation of one repository's remote-tracking display state from the
// branch metadata the sync-runs feed exposes. The Workspace Details status
// pill and the expanded tracking detail both use it, so they cannot disagree.
// Priority is review > blocked > behind > ahead > healthy; ahead and behind are
// binary signals, never commit counts.
//
// Ontology: g5.workspace.remote-ui.status (features/workspaces.g5.md)
// =============================================================================

import type {
  BranchTrackingRepo,
  WorkspaceMergeRunSummary,
} from "../../lib/types";

export type RemoteTrackingState =
  | "healthy"
  | "ahead"
  | "behind"
  | "review"
  | "blocked";

interface RemoteTrackingStatus {
  state: RemoteTrackingState;
  /** Raw `sync_paused_reason` when the repo is paused (review or blocked). */
  pauseReason?: string;
  /** Full-text status label for the pill. */
  label: string;
  /** The paused repo row, when any. */
  pausedRepo?: BranchTrackingRepo;
  /** The first repo row that is behind, when the state is `behind`. */
  behindRepo?: BranchTrackingRepo;
}

/** Human phrase for a `sync_paused_reason`, used in the detail header. */
export function describePauseReason(reason: string): string {
  if (reason.startsWith("merge_run:")) return "divergence — merge required";
  switch (reason) {
    case "diverged":
      return "divergence — merge required";
    case "watermark_violation":
      return "provider history was rewritten outside G5 — publication approval required";
    case "auth_required":
      return "OAuth credential missing or insufficient scope";
    case "push_failed":
      return "push to remote failed";
    case "fetch_failed":
      return "fetch from remote failed";
    case "org_binding_mismatch":
      return "cross-org binding mismatch";
    default:
      return reason;
  }
}

/**
 * A repo is "behind" when the advisory ping observed a remote tip that has
 * moved past the synchronization watermark (`baselineCommit`). Compared to the
 * baseline, NOT `headCommit`: under `push_mode = manual` the workspace can sit
 * ahead of the remote while the advisory still equals the baseline, and an
 * `advisory != head` test would mislabel that ahead state as behind.
 */
function isBehind(repo: BranchTrackingRepo): boolean {
  return (
    !!repo.remoteTipAdvisorySha &&
    !!repo.baselineCommit &&
    repo.remoteTipAdvisorySha !== repo.baselineCommit
  );
}

/**
 * A repo is "ahead" when the workspace head has advanced past the watermark —
 * local commits not yet on the remote. Normal under `push_mode = manual`.
 */
function isAhead(repo: BranchTrackingRepo): boolean {
  return !!repo.baselineCommit && repo.headCommit !== repo.baselineCommit;
}

/**
 * A provider-rewrite approval checkpoint: the repo is paused for
 * `watermark_violation` and has its own active `reset_below_baseline` run.
 */
function isResetBelowBaselineReview(
  repo: BranchTrackingRepo | undefined,
  activeMergeRuns: WorkspaceMergeRunSummary[],
): boolean {
  return (
    repo?.syncPausedReason === "watermark_violation" &&
    activeMergeRuns.some(
      (run) =>
        run.repoId === repo.repoId && run.phase === "reset_below_baseline",
    )
  );
}

function blockedLabel(reason: string): string {
  if (reason === "diverged" || reason.startsWith("merge_run:")) {
    return "Paused — merge required";
  }
  switch (reason) {
    case "auth_required":
      return "Authentication required";
    case "push_failed":
      return "Push failed";
    case "fetch_failed":
      return "Fetch failed";
    case "watermark_violation":
      return "Paused — provider history rewritten";
    case "org_binding_mismatch":
      return "Paused — cross-org binding mismatch";
    default:
      return `Paused — ${reason.replace(/_/g, " ")}`;
  }
}

/** Full-text pill label for a derived state. */
export function remoteTrackingLabel(
  state: RemoteTrackingState,
  pauseReason?: string,
): string {
  switch (state) {
    case "healthy":
      return "Tracking healthy";
    case "ahead":
      return "Ahead — local commits to push";
    case "behind":
      return "Remote moved — your branch is behind";
    case "review":
      return "Review required — approve rebased publication";
    case "blocked":
      return pauseReason ? blockedLabel(pauseReason) : "Paused";
  }
}

/**
 * Derive the status from repo rows already narrowed to `remote_tracking`
 * children in the caller's scope, plus their active merge runs. Returns `null`
 * when there is nothing to display.
 */
export function deriveRemoteTrackingStatus(
  repos: BranchTrackingRepo[],
  activeMergeRuns: WorkspaceMergeRunSummary[],
): RemoteTrackingStatus | null {
  if (repos.length === 0) return null;

  const pausedRepo = repos.find((repo) => repo.syncPausedReason);
  if (pausedRepo) {
    const pauseReason = pausedRepo.syncPausedReason ?? undefined;
    const state = isResetBelowBaselineReview(pausedRepo, activeMergeRuns)
      ? "review"
      : "blocked";
    return {
      state,
      pauseReason,
      label: remoteTrackingLabel(state, pauseReason),
      pausedRepo,
    };
  }
  // Behind (remote moved) outranks ahead: it is the more actionable warning.
  const behindRepo = repos.find(isBehind);
  if (behindRepo) {
    return {
      state: "behind",
      label: remoteTrackingLabel("behind"),
      behindRepo,
    };
  }
  const state = repos.some(isAhead) ? "ahead" : "healthy";
  // Rewritten history replaces, rather than extends, the provider branch.
  const label =
    state === "ahead" && repos.some((repo) => repo.rewritePending)
      ? "Ahead — rewritten history to push"
      : remoteTrackingLabel(state);
  return { state, label };
}
