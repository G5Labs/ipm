// =============================================================================
// G5 Client — Remote-tracking publish controls
// =============================================================================
//
// Per-workspace outbound-publish controls for remote-tracked branches:
//   * "Push to Remote" — the explicit PushToRemote action (the only
//     publication path in manual mode).
//   * Auto-push toggle — flips the per-workspace push_mode (manual | auto).
//
// PushToRemote is ASYNCHRONOUS: the request enqueues a `'push'` integration
// cycle per tracked branch; the worker performs the actual git push moments
// later, writes a `pushed` audit row, and advances the advisory (which flips
// the repo status pill to healthy). The button click therefore can only
// acknowledge that the push was *queued* — so we show a "publishing" notice
// and briefly burst-refetch the shared sync-runs feed so the result surfaces
// within a couple seconds instead of waiting for its 15s background poll
// (C30 — "how do I know it pushed? if it takes a while, how am I told?").
//
// SOT:
// - designs/REMOTE_REPO_TRACKING.md §6.2 (push_mode + Trigger C), §6.4 (advisory)
// - views/arch_views/remote_repo_tracking.arch_view.md D49, D66, D67
// - views/pm_views/remote_repo_tracking.pm_view.md D20
// - Chat session `remote-tracking-gap-closure-plan-2026-05-28`: C27, C28, C30
// =============================================================================

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useSyncWorkspace, useWorkspace } from "../../hooks/useWorkspace";
import { usePushToRemote, useSetPushMode } from "../../hooks/useRemoteTracking";
import { api } from "../../lib/api";
import type {
  BranchSyncRun,
  CoordinatedRepoEvidence,
  ListBranchSyncRunsResponse,
} from "../../lib/types";
import { CoordinatedRepoSyncModal } from "./CoordinatedRepoSyncModal";
import {
  coordinatedModalFromError,
  type CoordinatedRepoSyncModalState,
} from "./CoordinatedRepoSyncModalState";
import {
  ownerOrBaseAdmin,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

export interface RemoteTrackingActionsProps {
  workspaceId: string;
  /**
   * The per-child Auto-pull toggle, rendered directly under the Pull button so
   * the pull operations (Pull button + Auto-pull toggle) group together, above
   * the push operations (Push button + Auto-push toggle). Supplied by the
   * tracking popover; omitted when there is no per-repo auto-pull control.
   */
  autoPullSlot?: ReactNode;
}

/**
 * Delays (ms after enqueue) at which we refetch the sync-runs feed so the
 * worker's `pushed` result + the healthy status pill appear promptly. The
 * feed otherwise refreshes on its own 15s background poll.
 */
const SETTLE_REFETCH_DELAYS_MS = [1500, 3500, 6000, 9000];

type PushNoticeTone = "neutral" | "success" | "error";

interface PushNotice {
  tone: PushNoticeTone;
  message: string;
}

interface PushAttempt {
  startedAtMs: number;
  repos: CoordinatedRepoEvidence[];
}

const PUSH_ERROR_STATUSES = new Set([
  "auth_required",
  "fetch_failed",
  "push_failed",
]);

function repoLabel(repo: Pick<CoordinatedRepoEvidence, "repoName">): string {
  return `repo ${repo.repoName}`;
}

function formatList(values: string[]): string {
  if (values.length <= 1) return values[0] ?? "";
  if (values.length === 2) return `${values[0]} and ${values[1]}`;
  return `${values.slice(0, -1).join(", ")}, and ${values[values.length - 1]}`;
}

function formatQueuedPushNotice(
  enqueued: number,
  repos: CoordinatedRepoEvidence[],
): PushNotice {
  if (repos.length > 0) {
    return {
      tone: "neutral",
      message: `Queued push for ${formatList(repos.map(repoLabel))}.`,
    };
  }

  return {
    tone: "neutral",
    message:
      enqueued > 0
        ? `Queued ${enqueued} branch${enqueued === 1 ? "" : "es"} — publishing to the remote now. Watch Recent pull/push activity below; the status reads Tracking healthy once it lands.`
        : "Nothing to publish — no tracked branch has local commits ahead of the remote.",
  };
}

function latestPushRunForRepo(
  repo: CoordinatedRepoEvidence,
  runs: BranchSyncRun[],
  startedAtMs: number,
): BranchSyncRun | null {
  const matching = runs
    .filter((run) => {
      if (run.triggerKind !== "push") return false;
      if (run.repoId !== repo.repoId) return false;
      const runStartedAtMs = Date.parse(run.startedAt);
      if (!Number.isNaN(runStartedAtMs) && runStartedAtMs < startedAtMs) {
        return false;
      }
      return !repo.newCommit || run.newWorkspaceTip === repo.newCommit;
    })
    .sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt));
  return matching[0] ?? null;
}

function formatPushOutcomeNotice(
  attempt: PushAttempt,
  runs: BranchSyncRun[],
): PushNotice | null {
  const outcomes = attempt.repos.map((repo) => ({
    repo,
    run: latestPushRunForRepo(repo, runs, attempt.startedAtMs),
  }));

  if (outcomes.some((outcome) => !outcome.run)) return null;

  const pushed = outcomes
    .filter((outcome) => outcome.run?.status === "pushed")
    .map((outcome) => repoLabel(outcome.repo));
  const unchanged = outcomes
    .filter((outcome) => outcome.run?.status === "noop")
    .map((outcome) => repoLabel(outcome.repo));
  const incoming = outcomes
    .filter(
      (outcome) =>
        outcome.run?.status === "fetched" || outcome.run?.status === "diverged",
    )
    .map((outcome) => repoLabel(outcome.repo));
  const failed = outcomes.filter(
    (outcome) => !!outcome.run && PUSH_ERROR_STATUSES.has(outcome.run.status),
  );

  if (failed.length > 0) {
    return {
      tone: "error",
      message: failed
        .map((outcome) => {
          const detail =
            outcome.run?.errorDetail ?? outcome.run?.status ?? "push failed";
          return `Push failed for ${repoLabel(outcome.repo)}: ${detail}.`;
        })
        .join(" "),
    };
  }

  const parts: string[] = [];
  if (pushed.length > 0) {
    parts.push(`Push succeeded for ${formatList(pushed)}.`);
  }
  if (unchanged.length > 0) {
    parts.push(`No changes to push in ${formatList(unchanged)}.`);
  }
  if (incoming.length > 0) {
    parts.push(
      `Remote changes available for ${formatList(incoming)}. Reconcile them in the running workspace before publishing.`,
    );
  }
  return parts.length > 0
    ? {
        tone: incoming.length > 0 ? "neutral" : "success",
        message: parts.join(" "),
      }
    : null;
}

function pushNoticeClassName(tone: PushNoticeTone): string {
  switch (tone) {
    case "success":
      return "text-[var(--g5-ok)]";
    case "error":
      return "text-[var(--g5-blocked)]";
    default:
      return "text-[var(--g5-text2)]";
  }
}

/**
 * Renders the per-workspace "Push to Remote" button and the manual/auto
 * push-mode toggle. The current mode is read from the workspace detail
 * (`useWorkspace`); both controls drive the dedicated remote-tracking
 * endpoints and invalidate the workspace + sync-runs caches on success.
 */
export function RemoteTrackingActions({
  workspaceId,
  autoPullSlot,
}: RemoteTrackingActionsProps) {
  const ws = useWorkspace(workspaceId);
  const sync = useSyncWorkspace(workspaceId);
  const push = usePushToRemote(workspaceId);
  const setMode = useSetPushMode(workspaceId);
  const queryClient = useQueryClient();

  // Acknowledgement state for the async publish (see file header).
  const [pushNotice, setPushNotice] = useState<PushNotice | null>(null);
  const [pullNotice, setPullNotice] = useState<string | null>(null);
  const [syncModal, setSyncModal] =
    useState<CoordinatedRepoSyncModalState | null>(null);
  const timersRef = useRef<number[]>([]);
  const pushAttemptIdRef = useRef(0);
  const pullInFlightRef = useRef(false);
  const [pullSubmitting, setPullSubmitting] = useState(false);

  const clearTimers = useCallback(() => {
    timersRef.current.forEach((id) => window.clearTimeout(id));
    timersRef.current = [];
  }, []);
  // Cancel any in-flight settle-refetch timers on unmount.
  useEffect(() => clearTimers, [clearTimers]);

  const pushMode = ws.status === "ok" ? ws.data.pushMode : "manual";
  const isBase = ws.status === "ok" && ws.data.isBase;
  const isAuto = pushMode === "auto";
  const modeKnown = ws.status === "ok";
  // Fetch, push and their toggles are the owner's and check ownership alone
  // on the server, so a read-only or freezing owner keeps them
  // (`g5.workspace.ui.reader-publication`); a reader sees none of them. On
  // Base, which shows Pull only, an organization administrator pulls too
  // (`g5.workspace.remote.base-admin-pull`).
  const gate = useWorkspaceLiveGate();
  const offered = gate ? ownerOrBaseAdmin(gate) : true;

  const refreshPushOutcome = useCallback(
    async (attempt: PushAttempt, attemptId: number) => {
      if (attempt.repos.length === 0) return;
      const branchIds = Array.from(
        new Set(
          attempt.repos
            .map((repo) => repo.repoId)
            .filter((branchId): branchId is string => !!branchId),
        ),
      );
      if (branchIds.length === 0) return;

      try {
        const responses = await Promise.all(
          branchIds.map((branchId) =>
            api.get<ListBranchSyncRunsResponse>(
              `/api/v1/workspaces/${workspaceId}/repos/${branchId}/sync_runs?page=1&per_page=50`,
            ),
          ),
        );
        if (pushAttemptIdRef.current !== attemptId) return;
        const runs = responses.flatMap((response) => response.runs.data);
        const notice = formatPushOutcomeNotice(attempt, runs);
        if (notice) {
          setPushNotice(notice);
        }
      } catch (error) {
        if (pushAttemptIdRef.current !== attemptId) return;
        setPushNotice({
          tone: "error",
          message:
            error instanceof Error
              ? `Could not load push result: ${error.message}`
              : "Could not load push result.",
        });
      }
    },
    [workspaceId],
  );

  const handlePull = useCallback(async () => {
    if (pullInFlightRef.current) return;
    pullInFlightRef.current = true;
    setPullSubmitting(true);
    try {
      const result = await sync.mutateAsync();
      // Base has no VM checkout to pull into; its accepted updates are final.
      const queued = isBase
        ? "Remote fetch queued. G5 will accept fast-forward updates into Base."
        : "Remote fetch queued. G5 will accept fast-forward updates; use git pull in the workspace to pick them up. Diverged branches require rebase. Workspace files stay unchanged until you pull.";
      // An explicit Pull imports a held repository anyway, alone
      // (g5.governance.remote.pull).
      const held = (result?.details ?? [])
        .filter((detail) => detail.heldBy)
        .map(
          (detail) =>
            `workspace PR “${detail.heldBy?.title}” is merged on the provider for one repository but not all of them, so this pull imports that repository alone`,
        );
      setPullNotice(
        held.length > 0 ? `${queued} Note: ${held.join("; ")}.` : queued,
      );
      queryClient.invalidateQueries({ queryKey: ["branch-sync-runs"] });
    } catch (error) {
      setPullNotice(null);
      setSyncModal(coordinatedModalFromError(error, "Pull from remote failed"));
    } finally {
      pullInFlightRef.current = false;
      setPullSubmitting(false);
    }
  }, [sync, queryClient, isBase]);

  const handlePush = useCallback(async () => {
    const startedAtMs = Date.now() - 1000;
    try {
      const res = await push.mutateAsync();
      const attemptId = startedAtMs;
      pushAttemptIdRef.current = attemptId;
      const repos = res.coordinated?.repos ?? [];
      const attempt = { startedAtMs, repos };
      setPushNotice(formatQueuedPushNotice(res.enqueued, repos));
      clearTimers();
      // Burst-refetch the sync-runs feed (shared react-query cache with the
      // repo status pill) so the `pushed` row + healthy pill land quickly.
      for (const ms of SETTLE_REFETCH_DELAYS_MS) {
        timersRef.current.push(
          window.setTimeout(() => {
            queryClient.invalidateQueries({ queryKey: ["branch-sync-runs"] });
            void refreshPushOutcome(attempt, attemptId);
          }, ms),
        );
      }
    } catch (error) {
      setPushNotice(null);
      setSyncModal(coordinatedModalFromError(error, "Push to remote failed"));
    }
  }, [push, clearTimers, queryClient, refreshPushOutcome]);

  const pullSection = (
    <>
      <button
        type="button"
        data-testid="pull-from-remote-button"
        disabled={pullSubmitting || sync.isLoading}
        onClick={handlePull}
        className="w-full rounded bg-[var(--g5-text0)] px-2 py-1 font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
      >
        {pullSubmitting || sync.isLoading ? "Fetching..." : "Fetch from Remote"}
      </button>
      {pullNotice && (
        <p
          data-testid="pull-complete-notice"
          role="status"
          className="mt-1 text-[var(--g5-text2)]"
        >
          {pullNotice}
        </p>
      )}
      {autoPullSlot}
    </>
  );

  if (!offered) return null;

  return (
    <div
      data-testid="remote-tracking-actions"
      className="mt-2 border-t border-[var(--g5-stroke)] pt-2 text-xs"
    >
      {!modeKnown ? (
        <button
          type="button"
          data-testid="remote-action-loading-button"
          disabled
          className="w-full rounded bg-[var(--g5-text0)] px-2 py-1 font-medium text-[var(--g5-bg0)] opacity-50"
        >
          Loading...
        </button>
      ) : isBase ? (
        // Base follows its provider and never publishes to it
        // (g5.workspace.remote.base-parity): pull operations only.
        pullSection
      ) : (
        // Other workspaces expose BOTH directions symmetrically (#1198),
        // grouped so a button sits with its toggle at minimal spacing and the
        // pull section is separated from the push section by a slightly larger
        // gap: Pull button, Auto-pull toggle, Push button, Auto-push toggle.
        <>
          {pullSection}

          {/* Push section — slightly more spacing above than the toggle gap */}
          <button
            type="button"
            data-testid="push-to-remote-button"
            disabled={push.isLoading}
            onClick={handlePush}
            className="mt-3 w-full rounded bg-[var(--g5-text0)] px-2 py-1 font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
          >
            {push.isLoading ? "Queuing push..." : "Push to Remote"}
          </button>
          {pushNotice && (
            <p
              data-testid="push-queued-notice"
              role="status"
              className={`mt-1 ${pushNoticeClassName(pushNotice.tone)}`}
            >
              {pushNotice.message}
            </p>
          )}
          <label
            className="mt-1 flex items-center gap-2"
            data-testid="auto-push-mode-label"
          >
            <input
              type="checkbox"
              data-testid="auto-push-toggle"
              checked={isAuto}
              disabled={setMode.isLoading || !modeKnown}
              onChange={(e) =>
                setMode.mutate(e.target.checked ? "auto" : "manual")
              }
            />
            <span>Auto-push to remote</span>
          </label>
          <p
            data-testid="push-mode-hint"
            className="mt-1 text-[var(--g5-text2)]"
          >
            {isAuto
              ? "Auto: requires a GitHub App with write access. Push to Remote can use your personal connection."
              : "Manual: pushes save to G5. Use Push to Remote to update the provider."}
          </p>
        </>
      )}

      {setMode.error && (
        <p
          data-testid="set-mode-error"
          className="mt-1 text-[var(--g5-blocked)]"
        >
          {setMode.error.message}
        </p>
      )}
      {syncModal && (
        <CoordinatedRepoSyncModal
          {...syncModal}
          onClose={() => setSyncModal(null)}
        />
      )}
    </div>
  );
}
