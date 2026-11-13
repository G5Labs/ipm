// =============================================================================
// G5 — BranchTrackingPopover (tracking detail body)
// =============================================================================
//
// The remote-tracking detail for one repository, rendered in page flow when
// the Workspace Details card is expanded (`BranchTrackingInline`). Sections,
// each gated on backend state:
//
//   1. Header: paused reason or healthy + last non-`noop` action timestamp.
//   2. Merge-required: one card per active `workspace_merge_runs` row with
//      `source_kind = 'remote_tracking'`. Surfaces the three SHAs
//      (workspace tip, mirror tip, baseline) and its resolution actions:
//      Resolve with agent, or adopting the provider history instead. Base
//      offers only adopting, which it calls Accept provider history.
//   3. Retry: shown when the latest sync-runs status is `auth_required`,
//      `push_failed`, or `fetch_failed`. Calls `POST .../sync_runs/retry`.
//   4. Pull/Push with the auto-pull/auto-push controls (pull only on Base),
//      and recent activity.
//
// This body is the only place tracking mutations render (the card head holds
// just the status pill and expander). It stays open after an action so its
// feedback remains visible.
//
// Ontology: g5.workspace.remote-ui.detail, g5.workspace.remote-ui.actions
// =============================================================================

import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { Check, Copy } from "lucide-react";
import { coordinatedModalFromError } from "./CoordinatedRepoSyncModalState";
import {
  branchMergeRunsQueryKey,
  useActiveBranchMergeRuns,
  useApprovedRebasePublish,
  useBranchSyncRuns,
  useCompleteBranchMergeRun,
  useRetryBranchSyncRuns,
  useStartBranchMergeRun,
} from "../../hooks/useRemoteTracking";
import { RemoteTrackingActions } from "./RemoteTrackingActions";
import { useWorkspace } from "../../hooks/useWorkspace";
import { BranchAutoPullControl } from "./BranchAutoPullControl";
import { RemoteConflictAgentLauncher } from "./RemoteConflictAgentLauncher";
import { ProviderHistoryAdoption } from "./ProviderHistoryAdoption";
import { useWorkspaceLiveGate } from "../../lib/workspaceLiveGate";
import { workspacePagePath } from "./reviewWorkspace";
import { chatLandingQuery } from "../../lib/workspaceLanding";
import {
  describePauseReason,
  deriveRemoteTrackingStatus,
  type RemoteTrackingState,
} from "./remoteTrackingStatus";
import type {
  AgentLaunchSelectionV1,
  BranchSyncRun,
  BranchTrackingRepo,
  WorkspaceMergeRunSummary,
} from "../../lib/types";

// ---------------------------------------------------------------------------
// Status mapping
// ---------------------------------------------------------------------------

const RETRYABLE_LAST_STATUSES = new Set([
  "auth_required",
  "push_failed",
  "fetch_failed",
]);

function describePhase(phase: string): string {
  switch (phase) {
    case "workspace_rebase_preflight":
      return "Workspace and remote diverged";
    case "reset_below_baseline":
      return "Rebased publication needs approval";
    default:
      return phase.replace(/_/g, " ");
  }
}

function shortenSha(sha?: string): string {
  if (!sha) return "—";
  return sha.length > 7 ? sha.slice(0, 7) : sha;
}

function formatTimestamp(iso?: string): string {
  if (!iso) return "";
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

function isNonNoop(run: BranchSyncRun): boolean {
  return run.status !== "noop";
}

const SYNC_RUN_SUCCESS_STATUSES = new Set([
  "pushed",
  "imported",
  "fetched",
  "advisory_updated",
  "resolved",
]);
const SYNC_RUN_ERROR_STATUSES = new Set([
  "diverged",
  "auth_required",
  "push_failed",
  "fetch_failed",
]);
const SYNC_RUN_BACKGROUND_TRIGGERS = new Set([
  "background_inbound",
  "ping",
  "webhook",
]);

/** Human label for a sync-run outcome (pull/push activity feedback). */
function syncRunLabel(run: BranchSyncRun): string {
  switch (run.status) {
    case "pushed":
      return "Pushed to remote";
    case "fetched":
      return "Remote refs refreshed; workspace unchanged";
    case "imported":
      return "Pulled new commits";
    case "advisory_updated":
      return "Remote moved";
    case "resolved":
      return "Divergence resolved — tracking resumed";
    case "noop":
      return "No change";
    case "diverged":
      return "Diverged — needs reconciliation";
    case "dirty_skipped":
      return run.errorDetail === "merge_active"
        ? "Skipped — reconciliation in progress"
        : "Skipped — uncommitted work";
    case "auth_required":
      return "Authentication required";
    case "push_failed":
      return "Push failed";
    case "fetch_failed":
      return "Fetch failed";
    default:
      return run.status.replace(/_/g, " ");
  }
}

/** Short trigger context for a sync run. */
function syncRunTrigger(run: BranchSyncRun): string {
  switch (run.triggerKind) {
    case "manual":
      return "manual";
    case "commit":
      return "on commit";
    case "push":
      return "push";
    case "workspace_open":
      return "on open";
    case "background_inbound":
      return "background";
    case "ping":
      return "check";
    case "webhook":
      return "webhook";
    default:
      return run.triggerKind.replace(/_/g, " ");
  }
}

function syncRunToneClass(run: BranchSyncRun): string {
  if (SYNC_RUN_ERROR_STATUSES.has(run.status))
    return "text-[var(--g5-blocked)]";
  if (SYNC_RUN_SUCCESS_STATUSES.has(run.status)) return "text-[var(--g5-ok)]";
  return "text-[var(--g5-text2)]";
}

/** Header status-line color per derived state. */
const STATUS_LINE_CLASS: Record<RemoteTrackingState, string> = {
  review: "text-[var(--g5-warn)]",
  blocked: "text-[var(--g5-blocked)]",
  behind: "text-[var(--g5-warn)]",
  ahead: "text-[var(--g5-info)]",
  healthy: "text-[var(--g5-ok)]",
};

/**
 * A run worth showing in the activity list: any non-noop outcome, plus noop
 * outcomes from an explicit user action (manual pull, commit, open) — but not
 * the automatic background "checked, nothing changed" pings, which are noise.
 */
function isDisplayableSyncRun(run: BranchSyncRun): boolean {
  return (
    run.status !== "noop" || !SYNC_RUN_BACKGROUND_TRIGGERS.has(run.triggerKind)
  );
}

// ---------------------------------------------------------------------------
// Subcomponents
// ---------------------------------------------------------------------------

interface ShaRowProps {
  label: string;
  sha?: string;
}

function ShaRow({ label, sha }: ShaRowProps) {
  const [copied, setCopied] = useState(false);
  const [copyFailed, setCopyFailed] = useState(false);

  const handleCopy = async () => {
    if (!sha) return;
    try {
      await navigator.clipboard.writeText(sha);
      setCopied(true);
      setCopyFailed(false);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopyFailed(true);
      setTimeout(() => setCopyFailed(false), 3000);
    }
  };

  return (
    <div
      className="flex items-center justify-between gap-2 text-xs"
      data-testid={`sha-row-${label.toLowerCase().replace(/\s+/g, "-")}`}
    >
      <span className="text-[var(--g5-text2)]">{label}</span>
      <div className="flex items-center gap-1.5">
        <span className="font-mono text-[var(--g5-text0)]">
          {shortenSha(sha)}
        </span>
        {sha && (
          <button
            type="button"
            onClick={handleCopy}
            className="rounded p-0.5 hover:bg-[var(--g5-bg2)]"
            aria-label={`Copy ${label} SHA`}
            data-testid={`copy-${label.toLowerCase().replace(/\s+/g, "-")}`}
          >
            {copied ? (
              <Check size={12} className="text-[var(--g5-ok)]" />
            ) : (
              <Copy size={12} className="text-[var(--g5-text2)]" />
            )}
          </button>
        )}
        {copyFailed && (
          <span className="text-[10px] text-[var(--g5-blocked)]" role="status">
            Copy failed
          </span>
        )}
      </div>
    </div>
  );
}

/**
 * Opens the resolution agent's chat. The target is always the workspace root:
 * a relative `?session=` keeps the current file/doc/task path, which then opens
 * that content instead of the chat.
 */
function ResolutionSessionLink({
  sessionId,
  className,
}: {
  sessionId: string;
  className: string;
}) {
  const { org, workspace } = useParams<{ org: string; workspace: string }>();
  if (!org || !workspace) return null;
  return (
    <Link
      data-testid="open-resolution-session"
      className={className}
      to={`${workspacePagePath(org, workspace)}${chatLandingQuery(sessionId)}`}
    >
      Open resolution session
    </Link>
  );
}

interface MergeRunCardProps {
  run: WorkspaceMergeRunSummary;
  workspaceId: string;
  repoId: string;
  approvedPublishAllowed: boolean;
  approvedCheckpointIds: string[];
  showApprovedPublishAction: boolean;
  /** The owner resolves and marks runs; a reader only reads them. */
  mutationsAllowed?: boolean;
}

function MergeRunCard({
  run,
  workspaceId,
  repoId,
  approvedPublishAllowed,
  approvedCheckpointIds,
  showApprovedPublishAction,
  mutationsAllowed = true,
}: MergeRunCardProps) {
  const [confirming, setConfirming] = useState(false);
  const complete = useCompleteBranchMergeRun(workspaceId, repoId);
  const start = useStartBranchMergeRun(workspaceId, repoId);
  const approvedPublish = useApprovedRebasePublish(workspaceId);
  const approvedPublishInFlightRef = useRef(false);
  const startInFlightRef = useRef(false);
  const [startSubmitting, setStartSubmitting] = useState(false);

  const workspaceTip =
    typeof run.sourceHeads?.workspace_tip === "string"
      ? run.sourceHeads.workspace_tip
      : undefined;
  const mirrorTip =
    typeof run.sourceHeads?.mirror_tip === "string"
      ? run.sourceHeads.mirror_tip
      : undefined;
  const baseline =
    typeof run.targetHeads?.baseline === "string"
      ? run.targetHeads.baseline
      : undefined;
  const checkpointWasRefreshed =
    run.targetHeads?.refresh_reason === "provider_descendant_advanced";

  // Hide both CTAs once the agent is mid-merge. `resolutionSessionId` is null
  // before start; populated immediately after.
  const inProgress =
    run.status === "running" ||
    !!["queued", "running", "waiting"].includes(run.resolutionTaskStatus ?? "");
  const isResetBelowBaseline = run.phase === "reset_below_baseline";
  const canApprovedPublish = isResetBelowBaseline && approvedPublishAllowed;
  // Adopting is the alternative to an agent for an open divergence run that
  // has no resolution task yet; a provider-rewrite checkpoint keeps its own
  // approval (g5.workspace.remote.adopt-provider-history).
  const adoptable =
    run.phase === "workspace_rebase_preflight" &&
    !run.resolutionTaskId &&
    !run.resolutionSessionId;

  // Ownership can move while the confirmation is open; the former owner
  // gets no actionable confirmation (`g5.workspace.ui.reader-publication`).
  useEffect(() => {
    if (!mutationsAllowed) setConfirming(false);
  }, [mutationsAllowed]);
  const handleConfirm = async () => {
    if (!mutationsAllowed) return;
    try {
      await complete.mutateAsync(run.id);
      setConfirming(false);
    } catch {
      // Hook captures the error on `complete.error`.
    }
  };

  const handleStart = async (agentLaunch: AgentLaunchSelectionV1) => {
    if (startInFlightRef.current) return;
    startInFlightRef.current = true;
    setStartSubmitting(true);
    try {
      await start.mutateAsync({ mergeRunId: run.id, agentLaunch });
    } catch {
      // Hook captures the error on `start.error`.
    } finally {
      startInFlightRef.current = false;
      setStartSubmitting(false);
    }
  };

  const handleApprovedPublish = async () => {
    if (approvedPublishInFlightRef.current) return;
    approvedPublishInFlightRef.current = true;
    try {
      await approvedPublish.mutateAsync(approvedCheckpointIds);
    } catch {
      // Hook captures the error on `approvedPublish.error`.
    } finally {
      approvedPublishInFlightRef.current = false;
    }
  };

  const approvedPublishError = approvedPublish.error
    ? coordinatedModalFromError(
        approvedPublish.error,
        "Rebase publication approval blocked",
      ).message
    : null;

  return (
    <div
      data-testid={`merge-run-card-${run.id}`}
      data-tone={canApprovedPublish ? "approval" : "blocked"}
      className={
        canApprovedPublish
          ? "rounded border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-2"
          : "rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2"
      }
    >
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="text-sm font-medium text-[var(--g5-text0)]">
          {describePhase(run.phase)}
        </span>
        <span
          className={
            canApprovedPublish
              ? "text-[10px] uppercase tracking-wide text-[var(--g5-warn)]"
              : "text-[10px] uppercase tracking-wide text-[var(--g5-blocked)]"
          }
        >
          {run.status.replace(/_/g, " ")}
        </span>
      </div>
      {run.blockingReason && (
        <p
          data-testid={`merge-run-blocking-${run.id}`}
          role="status"
          className="mb-2 text-[11px] leading-snug text-[var(--g5-text0)]"
        >
          {run.blockingReason}
        </p>
      )}
      {canApprovedPublish ? (
        <details
          data-testid={`merge-run-details-${run.id}`}
          className="mb-2 text-xs text-[var(--g5-text1)]"
        >
          <summary className="cursor-pointer select-none text-[11px] text-[var(--g5-text1)]">
            Details
          </summary>
          <div className="mt-1 space-y-1">
            <ShaRow label="Workspace tip" sha={workspaceTip} />
            <ShaRow label="Mirror tip" sha={mirrorTip} />
            <ShaRow label="Baseline" sha={baseline} />
          </div>
        </details>
      ) : (
        <div className="mb-2 space-y-1">
          <ShaRow label="Workspace tip" sha={workspaceTip} />
          <ShaRow label="Mirror tip" sha={mirrorTip} />
          <ShaRow label="Baseline" sha={baseline} />
        </div>
      )}

      {inProgress ? (
        <div
          data-testid={`merge-run-in-progress-${run.id}`}
          className="rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-bg1)] px-2 py-1.5 text-xs text-[var(--g5-text0)]"
        >
          <span className="font-medium">Merge in progress</span>
          <span className="text-[var(--g5-text2)]">
            {" "}
            — agent working in this workspace
          </span>
          {run.resolutionSessionId && (
            <ResolutionSessionLink
              sessionId={run.resolutionSessionId}
              className="ml-2 underline"
            />
          )}
        </div>
      ) : canApprovedPublish ? (
        <div className="space-y-1.5">
          {checkpointWasRefreshed && (
            <p
              data-testid={`approval-checkpoint-refreshed-${run.id}`}
              role="status"
              aria-live="polite"
              className="rounded border border-[var(--g5-warn)]/40 bg-[var(--g5-bg1)] px-2 py-1.5 text-[11px] leading-snug text-[var(--g5-text0)]"
            >
              The provider branch moved forward after the previous approval
              checkpoint. G5 preserved that checkpoint for audit and created
              this one at the current provider tip. Approve again to continue.
            </p>
          )}
          <p className="text-[11px] leading-snug text-[var(--g5-text1)]">
            This checkpoint identifies the exact rebased candidate and provider
            tip that will be published. Approval applies the candidate through
            G5&apos;s normal workspace finalizer, then publishes it as a
            fast-forward update.
          </p>
          {showApprovedPublishAction && (
            <button
              type="button"
              data-testid={`publish-approved-rebase-${run.id}`}
              onClick={handleApprovedPublish}
              disabled={approvedPublish.isLoading}
              className="w-full rounded bg-[var(--g5-info)] px-2 py-1 text-xs font-medium text-white hover:brightness-95 disabled:opacity-50"
            >
              {approvedPublish.isLoading
                ? "Approving…"
                : approvedCheckpointIds.length > 1
                  ? `Approve ${approvedCheckpointIds.length} rebased publications`
                  : "Approve rebased publication"}
            </button>
          )}
          {showApprovedPublishAction && approvedPublishError && (
            <p
              data-testid={`publish-blocked-${run.id}`}
              role="alert"
              className="text-[11px] text-[var(--g5-text0)]"
            >
              Approval blocked: {approvedPublishError}. Fix the reported issue
              and approve again.
            </p>
          )}
        </div>
      ) : isResetBelowBaseline ? (
        <div
          data-testid={`approved-rebase-publish-unavailable-${run.id}`}
          className="rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-bg1)] px-2 py-1.5 text-xs text-[var(--g5-text0)]"
        >
          Approved rebase publishing requires this repo to be paused for a
          confirmed reset-below-baseline approval checkpoint.
        </div>
      ) : !confirming ? (
        <div className="space-y-1.5">
          {run.resolutionSessionId ? (
            <ResolutionSessionLink
              sessionId={run.resolutionSessionId}
              className="block text-xs underline"
            />
          ) : (
            <RemoteConflictAgentLauncher
              workspaceId={workspaceId}
              launchId={run.id}
              onStart={handleStart}
              starting={startSubmitting || start.isLoading}
            />
          )}
          {adoptable && (
            <div
              data-testid={`adopt-provider-history-${run.id}`}
              className="space-y-1.5 border-t border-[var(--g5-blocked)]/25 pt-1.5"
            >
              <p className="text-[11px] leading-snug text-[var(--g5-text1)]">
                Or adopt the provider history instead, discarding this
                workspace&apos;s commits the provider does not have.
              </p>
              <ProviderHistoryAdoption
                workspaceId={workspaceId}
                repoId={repoId}
                isBase={false}
              />
            </div>
          )}
          {mutationsAllowed && (
            <button
              type="button"
              data-testid={`mark-resolved-${run.id}`}
              onClick={() => setConfirming(true)}
              className="w-full rounded border border-[var(--g5-blocked)]/40 bg-[var(--g5-bg1)] px-2 py-1 text-xs font-medium text-[var(--g5-blocked)] hover:bg-[var(--g5-blocked)]/10"
            >
              Mark resolved
            </button>
          )}
          {start.error && (
            <p className="text-[11px] text-[var(--g5-blocked)]">
              Failed to start: {start.error.message}
            </p>
          )}
        </div>
      ) : (
        <div
          data-testid={`mark-resolved-confirm-${run.id}`}
          className="space-y-1.5"
        >
          <p className="text-[11px] leading-snug text-[var(--g5-text0)]">
            Confirm after you or the resolution agent have rebased and pushed
            the resolved tip in the workspace. The next integration cycle will
            push the resolved tip to the remote.
          </p>
          <div className="flex gap-1.5">
            <button
              type="button"
              data-testid="branch-tracking-mark-resolved-confirm"
              onClick={handleConfirm}
              disabled={complete.isLoading || !mutationsAllowed}
              className="flex-1 rounded bg-[var(--g5-text0)] px-2 py-1 text-xs font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
            >
              {complete.isLoading ? "Marking…" : "Confirm resolved"}
            </button>
            <button
              type="button"
              data-testid="branch-tracking-mark-resolved-cancel"
              onClick={() => setConfirming(false)}
              disabled={complete.isLoading}
              className="rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
            >
              Cancel
            </button>
          </div>
          {complete.error && (
            <p className="text-[11px] text-[var(--g5-blocked)]">
              Failed to mark resolved: {complete.error.message}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Detail body
// ---------------------------------------------------------------------------

function PausedDivergenceResolution({
  workspaceId,
  repoId,
}: {
  workspaceId: string;
  repoId: string;
}) {
  const start = useStartBranchMergeRun(workspaceId, repoId);
  const inFlight = useRef(false);
  const [submitting, setSubmitting] = useState(false);
  const handleStart = async (agentLaunch: AgentLaunchSelectionV1) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setSubmitting(true);
    try {
      await start.mutateAsync({ agentLaunch });
    } catch {
      // The hook retains the error so the owner can correct it and retry.
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  };
  return (
    <section
      data-testid="popover-paused-divergence-resolution"
      className="space-y-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-blocked)]/10 px-3 py-2"
    >
      <p className="text-xs text-[var(--g5-text0)]">
        Rebase your workspace commits onto the provider branch with an agent in
        the running workspace.
      </p>
      <RemoteConflictAgentLauncher
        workspaceId={workspaceId}
        launchId={`repo-${repoId}`}
        onStart={handleStart}
        starting={submitting || start.isLoading}
      />
      {start.error && (
        <p role="alert" className="text-[11px] text-[var(--g5-blocked)]">
          Failed to start: {start.error.message}
        </p>
      )}
    </section>
  );
}

/**
 * Base's only reconciliation (g5.workspace.remote.base-provider-rewrite): its
 * owner or an organization administrator adopts the provider history.
 */
function BaseProviderHistoryAcceptance({
  workspaceId,
  repoId,
}: {
  workspaceId: string;
  repoId: string;
}) {
  return (
    <section
      data-testid="popover-base-history-paused"
      className="space-y-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-blocked)]/10 px-3 py-2"
    >
      <p className="text-xs leading-snug text-[var(--g5-text0)]">
        Base&apos;s history differs from the provider branch. Base has no
        workspace VM to reconcile it; you can adopt the provider history
        instead.
      </p>
      <ProviderHistoryAdoption
        workspaceId={workspaceId}
        repoId={repoId}
        isBase
      />
    </section>
  );
}

/** Full-width in-flow container; no floating, shadow or portal chrome. */
const BODY_CLASS =
  "w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-sm text-[var(--g5-text0)]";

export interface BranchTrackingPopoverProps {
  workspaceId: string;
  repoId: string;
}

export function BranchTrackingPopover({
  workspaceId,
  repoId,
}: BranchTrackingPopoverProps) {
  const syncState = useBranchSyncRuns(workspaceId, repoId, { perPage: 20 });
  const mergeState = useActiveBranchMergeRuns(workspaceId, repoId);
  const retry = useRetryBranchSyncRuns(workspaceId, repoId);
  const workspace = useWorkspace(workspaceId);
  // Base has no VM to resolve divergence or prepare a rebased candidate
  // (g5.workspace.remote.base-parity), so it never offers those actions.
  const isBase = workspace.status === "ok" && workspace.data.isBase;
  // Retry, approval and resolution are the owner's; readers keep the status
  // and history (`g5.workspace.ui.reader-publication`).
  const gate = useWorkspaceLiveGate();
  const canMutate = gate ? gate.owner : true;
  const queryClient = useQueryClient();
  const retryInFlightRef = useRef(false);
  const [retrySubmitting, setRetrySubmitting] = useState(false);

  // Explicit integration can open a resolution record before the separate
  // merge-runs poll catches up. Refetch once to pick it up promptly. Automatic
  // auto-push divergence may deliberately have no record; that state gets its
  // own explicit resolution action below.
  const divergedRepoPresent =
    syncState.status === "ok" &&
    syncState.data.branch.repos.some(
      (r) =>
        r.trackingMode === "remote_tracking" &&
        r.repoId === repoId &&
        (r.syncPausedReason === "diverged" ||
          r.syncPausedReason === "watermark_violation"),
    );
  const mergeRunsEmpty =
    mergeState.status === "ok" && mergeState.data.items.length === 0;
  // Ref guard so we refetch exactly once per diverged-without-run episode — a
  // background refetch keeps `status: "ok"` with the stale (empty) list, so
  // without the guard the effect would re-fire every render until the run lands.
  const kickedMergeRefetchRef = useRef(false);
  useEffect(() => {
    if (divergedRepoPresent && mergeRunsEmpty) {
      if (!kickedMergeRefetchRef.current) {
        kickedMergeRefetchRef.current = true;
        queryClient.invalidateQueries({
          queryKey: branchMergeRunsQueryKey(workspaceId, repoId),
        });
      }
    } else {
      // Run appeared (or the branch left diverged): reset so a future
      // divergence re-arms the one-shot refetch.
      kickedMergeRefetchRef.current = false;
    }
  }, [divergedRepoPresent, mergeRunsEmpty, queryClient, workspaceId, repoId]);

  // `BranchTrackingInline` mounts this body only once both feeds have loaded
  // and a remote-tracking row exists; it owns the loading/unavailable states.
  if (syncState.status !== "ok") return null;

  const remoteTracking: BranchTrackingRepo[] =
    syncState.data.branch.repos.filter(
      (r) => r.trackingMode === "remote_tracking" && r.repoId === repoId,
    );

  // A half-merged workspace PR holds this repository's auto-pull
  // (g5.governance.remote.pull).
  const importHeldBy = remoteTracking[0]?.importHeldBy;

  const runs = syncState.data.runs.data.filter((r) =>
    remoteTracking.some((rt) => rt.repoId === r.repoId),
  );

  const scopedActiveMergeRuns: WorkspaceMergeRunSummary[] =
    mergeState.status === "ok"
      ? mergeState.data.items.filter((m) =>
          remoteTracking.some((rt) => rt.repoId === m.repoId),
        )
      : [];
  // Same derivation and wording as the card's status pill.
  const status = deriveRemoteTrackingStatus(
    remoteTracking,
    scopedActiveMergeRuns,
  );
  if (!status) return null;
  const { state, pausedRepo: paused, behindRepo } = status;
  const resetReview = state === "review";
  const behind = state === "behind";
  const ahead = state === "ahead";
  const coordinatedResetRuns: WorkspaceMergeRunSummary[] =
    resetReview && mergeState.status === "ok"
      ? mergeState.data.items
          .filter(
            (run) =>
              run.sourceKind === "remote_tracking" &&
              run.phase === "reset_below_baseline",
          )
          .sort((left, right) => left.id.localeCompare(right.id))
      : [];
  const activeMergeRuns = resetReview
    ? coordinatedResetRuns
    : scopedActiveMergeRuns;
  const approvedCheckpointIds = coordinatedResetRuns
    .map((run) => run.id)
    .sort((left, right) => left.localeCompare(right));
  const lastNonNoop = runs.find(isNonNoop);
  // Recent pull/push activity for the feedback list — user-visible runs, newest
  // first, excluding automatic background noop pings.
  const displayRuns = runs.filter(isDisplayableSyncRun).slice(0, 5);
  const retryEligible = remoteTracking.some(
    (repo) =>
      !!repo.latestRunStatus &&
      RETRYABLE_LAST_STATUSES.has(repo.latestRunStatus),
  );
  const handleRetry = async () => {
    if (retryInFlightRef.current) return;
    retryInFlightRef.current = true;
    setRetrySubmitting(true);
    try {
      await retry.mutateAsync(undefined);
    } catch {
      // Error surfaced via `retry.error`.
    } finally {
      retryInFlightRef.current = false;
      setRetrySubmitting(false);
    }
  };

  return (
    <div data-testid="branch-tracking-popover" className={BODY_CLASS}>
      <header className="border-b border-[var(--g5-stroke)] px-3 py-2">
        <div className="flex items-center justify-between gap-2">
          <span
            className={`font-medium ${STATUS_LINE_CLASS[state]}`}
            data-testid="popover-status-line"
            data-state={state}
          >
            {status.label}
          </span>
        </div>
        {paused?.syncPausedReason && (
          <p
            className="mt-0.5 text-xs text-[var(--g5-text1)]"
            data-testid="popover-pause-reason"
          >
            {describePauseReason(paused.syncPausedReason)}
          </p>
        )}
        <p
          className="mt-0.5 text-[11px] text-[var(--g5-text2)]"
          data-testid="popover-last-action"
        >
          {lastNonNoop
            ? `Last action: ${lastNonNoop.status.replace(/_/g, " ")} at ${formatTimestamp(lastNonNoop.startedAt)}`
            : "No non-trivial sync activity recorded yet"}
        </p>
      </header>

      {behind && (
        <section
          data-testid="popover-remote-moved"
          className="border-b border-[var(--g5-stroke)] bg-[var(--g5-warn)]/10 px-3 py-2"
        >
          <p className="text-xs leading-snug text-[var(--g5-text0)]">
            {isBase
              ? "The provider has new commits. Base imports them when auto-pull is on, or use Fetch from Remote."
              : "Ask an agent in the running workspace to reconcile the delivered provider changes. Fetching does not change workspace files."}
          </p>
          {behindRepo?.remoteTipAdvisoryObservedAt && (
            <p className="mt-0.5 text-[11px] text-[var(--g5-text2)]">
              Observed {formatTimestamp(behindRepo.remoteTipAdvisoryObservedAt)}
            </p>
          )}
        </section>
      )}

      {importHeldBy && (
        <section
          data-testid="popover-import-held"
          className="border-b border-[var(--g5-stroke)] bg-[var(--g5-warn)]/10 px-3 py-2"
        >
          <p className="text-xs leading-snug text-[var(--g5-text0)]">
            Auto-pull is holding this repository: workspace PR “
            {importHeldBy.title}” is merged on the provider here but not for all
            of its repositories. It imports with that PR, or Pull imports this
            repository alone.
          </p>
        </section>
      )}

      {ahead && !isBase && (
        <section
          data-testid="popover-ahead"
          className="border-b border-[var(--g5-stroke)] bg-[var(--g5-info)]/10 px-3 py-2"
        >
          <p className="text-xs leading-snug text-[var(--g5-text0)]">
            Your branch has local commits not yet pushed to the remote. Use Push
            to publish them.
          </p>
        </section>
      )}

      {isBase && (resetReview || paused?.syncPausedReason === "diverged") && (
        <BaseProviderHistoryAcceptance
          workspaceId={workspaceId}
          repoId={repoId}
        />
      )}

      {!isBase && activeMergeRuns.length > 0 && (
        <section
          data-testid="popover-merge-required"
          className="space-y-2 px-3 py-2"
        >
          <h3 className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
            {resetReview ? "Rebase publication approval" : "Merge required"}
          </h3>
          {resetReview && approvedCheckpointIds.length > 1 && (
            <p
              data-testid="approval-checkpoint-set-summary"
              className="rounded border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-2 py-1.5 text-[11px] leading-snug text-[var(--g5-text0)]"
            >
              This coordinated action approves all{" "}
              {approvedCheckpointIds.length} displayed checkpoints together.
              Review each provider tip before continuing.
            </p>
          )}
          {activeMergeRuns.map((run, index) => (
            <MergeRunCard
              key={run.id}
              run={run}
              workspaceId={workspaceId}
              repoId={repoId}
              approvedPublishAllowed={
                resetReview && run.phase === "reset_below_baseline"
              }
              approvedCheckpointIds={approvedCheckpointIds}
              showApprovedPublishAction={
                canMutate && (!resetReview || index === 0)
              }
              mutationsAllowed={canMutate}
            />
          ))}
        </section>
      )}

      {!isBase &&
        paused?.syncPausedReason === "diverged" &&
        mergeState.status === "ok" &&
        scopedActiveMergeRuns.length === 0 && (
          <PausedDivergenceResolution
            workspaceId={workspaceId}
            repoId={repoId}
          />
        )}

      {retryEligible && !resetReview && canMutate && (
        <section
          data-testid="popover-retry"
          className="border-t border-[var(--g5-stroke)] px-3 py-2"
        >
          <button
            type="button"
            data-testid="popover-retry-button"
            onClick={handleRetry}
            disabled={retrySubmitting || retry.isLoading}
            className="w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs font-medium text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
          >
            {retrySubmitting || retry.isLoading ? "Retrying…" : "Retry sync"}
          </button>
          {retry.error && (
            <p className="mt-1 text-[11px] text-[var(--g5-blocked)]">
              Retry blocked: {retry.error.message}
            </p>
          )}
        </section>
      )}

      {remoteTracking.length > 0 && !resetReview && (
        <section
          data-testid="popover-publish"
          className="border-t border-[var(--g5-stroke)] px-3 py-2"
        >
          {/* Order: Pull button, Auto-pull toggle, Push button, Auto-push toggle.
              The per-child Auto-pull toggle is injected next to the Pull button so
              the pull operations group together, above the push operations.
              Base never publishes, so it shows only the pull operations. */}
          <RemoteTrackingActions
            workspaceId={workspaceId}
            autoPullSlot={
              <BranchAutoPullControl
                workspaceId={workspaceId}
                repoId={repoId}
                className="mt-1 text-xs"
              />
            }
          />
        </section>
      )}

      {displayRuns.length > 0 && (
        <section
          data-testid="popover-recent-activity"
          className="border-t border-[var(--g5-stroke)] px-3 py-2"
        >
          <p className="mb-1 text-[11px] font-medium text-[var(--g5-text2)]">
            Recent pull/push activity
          </p>
          <ul className="space-y-1" data-testid="popover-recent-activity-list">
            {displayRuns.map((run) => (
              <li
                key={run.id}
                data-testid={`sync-run-${run.id}`}
                className="flex items-start justify-between gap-2 text-[11px]"
              >
                <span className={syncRunToneClass(run)}>
                  {syncRunLabel(run)}
                  {run.errorDetail ? (
                    <span className="block text-[10px] text-[var(--g5-text2)]">
                      {run.errorDetail === "merge_active"
                        ? "Waiting for the active merge or repair to finish."
                        : run.errorDetail}
                    </span>
                  ) : null}
                </span>
                <span className="shrink-0 text-[10px] text-[var(--g5-text2)]">
                  {syncRunTrigger(run)} · {formatTimestamp(run.startedAt)}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
