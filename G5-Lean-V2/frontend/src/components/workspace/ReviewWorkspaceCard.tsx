// =============================================================================
// G5 Workspace — ReviewWorkspaceCard
// =============================================================================
// The Approvals page's "Review Workspace" section for a viewer who does not
// own the workspace. Start review forks the source into a reviewer-owned
// review workspace, resumes its VM and opens
// it. Later visits show the persisted review workspace with its VM state, and
// Reopen review resumes the same one. What the reviewer pushed lives in the
// Reviews panel above, alongside every other reviewer's suggestions.
// =============================================================================

import React from "react";
import { Link, useNavigate } from "react-router-dom";
import { ExternalLink, Loader2 } from "lucide-react";
import { Button, Chip, classNames } from "../ui/UI";
import {
  useStartWorkspaceReview,
  useWorkspaceReviewContext,
} from "../../hooks/useWorkspaceReview";
import { formatDateTime } from "../../lib/dateTime";
import type { WorkspaceReview } from "../../lib/types";
import { ReviewBadge, ReviewStripe } from "./ReviewBadge";
import {
  markReviewPromptSeed,
  markReviewResumeError,
  reviewVmChipState,
  reviewVmStatusLabel,
  workspacePagePath,
} from "./reviewWorkspace";

export interface ReviewWorkspaceCardProps {
  /** The workspace under review (the source). */
  workspaceId: string;
  workspaceName: string;
  orgSlug: string;
  compact?: boolean;
}

export const ReviewWorkspaceCard: React.FC<ReviewWorkspaceCardProps> = ({
  workspaceId,
  workspaceName,
  orgSlug,
  compact = false,
}) => {
  const navigate = useNavigate();
  const context = useWorkspaceReviewContext(workspaceId);
  const start = useStartWorkspaceReview(workspaceId);
  const review: WorkspaceReview | null =
    context.status === "ok" ? context.data.asSource : null;

  const handleStart = async () => {
    try {
      const result = await start.mutateAsync();
      // A fresh copy gets the review prompt seeded once on arrival; a
      // reopened one keeps whatever its chat already holds. Either way the
      // page surfaces a VM that could not be resumed (the fork succeeded).
      if (result.created) {
        markReviewPromptSeed(result.review.reviewWorkspaceId);
      }
      if (result.resumeError) {
        markReviewResumeError(
          result.review.reviewWorkspaceId,
          result.resumeError,
        );
      }
      navigate(workspacePagePath(orgSlug, result.review.reviewWorkspaceName));
    } catch {
      // The mutation error renders below.
    }
  };

  const busy = start.isLoading;
  const startLabel = review
    ? review.vmStatus === "running"
      ? "Open review workspace"
      : "Reopen review"
    : "Start review";
  // While Start runs, the button says what is happening and the line below
  // says what it involves, so a fork that takes a while is not a mystery.
  const busyLabel = review
    ? "Reopening your review workspace…"
    : "Setting up your review workspace…";
  const busyDetail = review
    ? "Starting your review assistant. Use Update in the review workspace to pick up the latest changes."
    : "Creating a workspace copy and getting your review assistant ready.";

  if (compact)
    return (
      <div className="text-xs">
        <Button
          kind="secondary"
          data-testid="review-workspace-start"
          disabled={busy || context.status !== "ok"}
          onClick={handleStart}
        >
          {busy ? busyLabel : review ? startLabel : "Start Review"}
        </Button>
        {start.error && <p role="alert">{start.error.message}</p>}
      </div>
    );

  return (
    <div
      data-testid="review-workspace-card"
      className={classNames(
        "overflow-hidden rounded-[var(--g5-radius-md)]",
        "border border-[var(--g5-active)]/40 bg-[var(--g5-bg1)]",
      )}
    >
      <ReviewStripe />
      <div className="space-y-3 p-4">
        {context.status === "loading" ? (
          <p className="text-sm text-[var(--g5-text2)]">
            Loading review workspace…
          </p>
        ) : context.status === "error" ? (
          <p className="text-sm text-[var(--g5-blocked)]">
            Could not load the review workspace: {context.error.message}
          </p>
        ) : review ? (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <ReviewBadge />
              <Link
                to={workspacePagePath(orgSlug, review.reviewWorkspaceName)}
                className="text-sm font-semibold text-[var(--g5-text0)] hover:underline"
                data-testid="review-workspace-link"
              >
                {review.reviewWorkspaceName}
              </Link>
              <Chip
                state={reviewVmChipState(review.vmStatus)}
                text={reviewVmStatusLabel(review.vmStatus)}
                data-testid="review-workspace-vm-status"
              />
            </div>
            <p className="text-xs text-[var(--g5-text2)]">
              Your copy of {workspaceName}, forked{" "}
              {formatDateTime(review.createdAt)}
              {review.lastFlushedAt
                ? ` · state saved ${formatDateTime(review.lastFlushedAt)}`
                : ""}
              . Files, chat and terminal history stay between reviews; the VM
              only runs while you review. Push review in the copy sends your
              changes here as suggestions; they appear under Reviews above.
            </p>
          </>
        ) : (
          <>
            <p className="text-sm font-semibold text-[var(--g5-text0)]">
              Review {workspaceName} in your own copy
            </p>
            <p className="text-xs text-[var(--g5-text2)]">
              You don&apos;t have write access to {workspaceName}. Start review
              forks it into a review workspace that is yours to edit, run and
              test. Nothing reaches {workspaceName} until you push the review,
              and then only as suggestions its owner accepts one by one.
            </p>
          </>
        )}

        {context.status === "ok" && (
          <div className="flex flex-wrap items-center gap-2">
            <Button
              kind="primary"
              data-testid="review-workspace-start"
              onClick={handleStart}
              disabled={busy}
            >
              {busy ? (
                <>
                  <Loader2
                    className="h-3.5 w-3.5 animate-spin"
                    aria-hidden="true"
                  />
                  {busyLabel}
                </>
              ) : (
                startLabel
              )}
            </Button>
            {busy && (
              <p
                data-testid="review-workspace-progress"
                role="status"
                className="basis-full text-xs text-[var(--g5-text2)]"
              >
                {busyDetail}
              </p>
            )}
            {review && (
              <Link
                to={workspacePagePath(orgSlug, review.reviewWorkspaceName)}
                className="inline-flex items-center gap-1 text-xs text-[var(--g5-text1)] hover:underline"
              >
                <ExternalLink size={12} aria-hidden="true" />
                Browse without starting
              </Link>
            )}
          </div>
        )}
        {start.error && (
          <p
            data-testid="review-workspace-start-error"
            className="text-xs text-[var(--g5-blocked)]"
          >
            {start.error.message}
          </p>
        )}
      </div>
    </div>
  );
};
