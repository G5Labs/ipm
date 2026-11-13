// =============================================================================
// G5 — ProviderHistoryAdoption (review, then adopt the provider history)
// =============================================================================
//
// The one review-and-confirm control for adopting a repository's provider
// history, shared by the Make Remote dialog after a paused binding and the
// repository card's paused detail:
//
//   - Base lands it server-side, for its owner or an organization
//     administrator (g5.workspace.remote.base-provider-rewrite,
//     g5.workspace.remote.base-admin-pull).
//   - Any other workspace adopts in the owner's running VM, so the confirm
//     waits on live attachment with its reason and Start
//     (g5.workspace.remote.adopt-provider-history, g5.workspace.ui.gates).
//
// The viewer reviews the provider tip and the local commits that would be
// discarded, then confirms that exact review. Nothing is pushed to the
// provider.
// =============================================================================

import { useEffect, useRef, useState } from "react";
import { useProviderHistoryAcceptance } from "../../hooks/useRemoteTracking";
import {
  GatedControl,
  ownerOrBaseAdmin,
  useLiveGateReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import type { ProviderHistoryPreview } from "../../lib/types";

const ADOPT_VERB = "adopt the provider history";

export interface ProviderHistoryAdoptionProps {
  workspaceId: string;
  repoId: string;
  /** Base adopts without a VM; every other workspace adopts in its VM. */
  isBase: boolean;
  /** Fetch the review on mount instead of waiting for the viewer to ask. */
  autoReview?: boolean;
  /** Called once the adoption is accepted, with the review it adopted. */
  onAdopted?: (reviewed: ProviderHistoryPreview) => void;
}

export function ProviderHistoryAdoption({
  workspaceId,
  repoId,
  isBase,
  autoReview = false,
  onAdopted,
}: ProviderHistoryAdoptionProps) {
  const { preview, accept } = useProviderHistoryAcceptance(workspaceId, repoId);
  const [reviewed, setReviewed] = useState<ProviderHistoryPreview | null>(null);
  const [adopted, setAdopted] = useState(false);
  // Base admits its owner or an organization administrator; elsewhere the
  // owner adopts (`g5.workspace.ui.reader-publication`). Readers keep the
  // status text and lose the controls, and an open review closes when the
  // authority moves away.
  const gate = useWorkspaceLiveGate();
  const allowed = gate ? (isBase ? ownerOrBaseAdmin(gate) : gate.owner) : true;
  // Off Base the reset runs in the owner's VM; Base has none to wait for.
  const liveReason = useLiveGateReason(ADOPT_VERB);
  const adoptBlocked = isBase ? null : liveReason;

  useEffect(() => {
    if (!allowed) setReviewed(null);
  }, [allowed]);

  const handleReview = async () => {
    if (!allowed) return;
    accept.reset();
    try {
      setReviewed(await preview.mutateAsync(undefined));
    } catch {
      // The hook retains the error for display.
    }
  };

  const autoReviewed = useRef(false);
  useEffect(() => {
    if (!autoReview || !allowed || autoReviewed.current) return;
    autoReviewed.current = true;
    void handleReview();
    // Runs once, when the control first appears for a viewer who may adopt.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoReview, allowed]);

  const handleConfirm = async () => {
    // A confirmation kept open through a gate change rechecks it here.
    if (!reviewed || !allowed || adoptBlocked !== null) return;
    try {
      await accept.mutateAsync(reviewed);
      setReviewed(null);
      setAdopted(true);
      onAdopted?.(reviewed);
    } catch {
      // A stale review or a refused checkout changes nothing; review again.
      setReviewed(null);
    }
  };

  const error = accept.error ?? preview.error;
  const noun = isBase ? "Base" : "local";
  return (
    <div data-testid="provider-history-adoption" className="space-y-2">
      {!allowed ? (
        <p
          data-testid="accept-provider-history-readonly"
          className="text-[11px] text-[var(--g5-text2)]"
        >
          {isBase
            ? "Only Base's owner or an organization administrator can adopt the provider history."
            : "Only the owner can adopt the provider history."}
        </p>
      ) : adopted ? (
        <p
          data-testid="accept-provider-history-adopted"
          role="status"
          className="text-xs leading-snug text-[var(--g5-text0)]"
        >
          {isBase
            ? "Adopted the provider history."
            : "Adopted the provider history. Tracking resumes once G5 confirms the reset."}
        </p>
      ) : reviewed ? (
        <div
          data-testid="accept-provider-history-preview"
          className="space-y-2"
        >
          {reviewed.initialOnly && (
            <span
              data-testid="accept-provider-history-recommended"
              className="inline-flex rounded border border-[var(--g5-ok)]/40 bg-[var(--g5-ok)]/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-[var(--g5-ok)]"
            >
              Recommended
            </span>
          )}
          <p className="text-xs leading-snug text-[var(--g5-text0)]">
            Reset {isBase ? "Base" : "this workspace's branch"} to provider
            commit{" "}
            <code className="font-mono">
              {reviewed.providerTip.slice(0, 7)}
            </code>
            {reviewed.remoteBranch && (
              <>
                {" "}
                on <code className="font-mono">{reviewed.remoteBranch}</code>
              </>
            )}
            .{" "}
            {reviewed.discardedCommits === 1
              ? `1 ${noun} commit`
              : `${reviewed.discardedCommits} ${noun} commits`}{" "}
            not on the provider will be discarded
            {isBase ? " from Base" : ""}.
            {reviewed.initialOnly &&
              " The discarded history is only the repository's initial commit."}{" "}
            Nothing is pushed to the provider.
          </p>
          <div className="flex flex-wrap gap-2">
            <GatedControl
              verb={ADOPT_VERB}
              reason={adoptBlocked}
              testId="accept-provider-history-confirm-hint"
            >
              <button
                type="button"
                data-testid="accept-provider-history-confirm"
                onClick={handleConfirm}
                disabled={accept.isLoading || adoptBlocked !== null}
                title={adoptBlocked ?? undefined}
                className="rounded bg-[var(--g5-blocked)] px-2 py-1 text-xs font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
              >
                {accept.isLoading
                  ? "Adopting…"
                  : "Discard and adopt provider history"}
              </button>
            </GatedControl>
            <button
              type="button"
              data-testid="accept-provider-history-cancel"
              onClick={() => setReviewed(null)}
              disabled={accept.isLoading}
              className="rounded border border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          data-testid="accept-provider-history-button"
          onClick={handleReview}
          disabled={preview.isLoading}
          className="w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] disabled:opacity-50"
        >
          {preview.isLoading
            ? "Checking provider…"
            : isBase
              ? "Accept provider history"
              : "Adopt provider history"}
        </button>
      )}
      {error && (
        <p role="alert" className="text-[11px] text-[var(--g5-blocked)]">
          {error.message}
        </p>
      )}
    </div>
  );
}
