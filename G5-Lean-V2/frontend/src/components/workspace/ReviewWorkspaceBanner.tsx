// =============================================================================
// G5 Workspace — ReviewWorkspaceBanner
// =============================================================================
// Rendered directly under the top bar of a review workspace (a reviewer-owned
// fork of the workspace under review). Four cues say "this is a review, not
// the real thing", none of which a normal workspace shows: the diagonal
// stripe, the Review badge, and the banner naming the source. The banner
// owns the Push review dialog, opened from the Review tab's one Push review
// button; Push review turns the committed difference between this copy and
// the source into suggestions on the source, which its owner accepts into
// the running source workspace one by one. Update from the source rebases
// this copy onto the source's latest changes inside the reviewer's own VM.
// =============================================================================

import React, { useEffect, useId, useRef, useState } from "react";
import { AlertTriangle, ArrowLeft, Loader2 } from "lucide-react";
import { Button, classNames } from "../ui/UI";
import { Modal } from "../ui/Modal";
import {
  usePublishWorkspaceReview,
  useReviewPreview,
  useReviewPublications,
} from "../../hooks/useWorkspaceReview";
import {
  useWorkspaceChangesAction,
  type WorkspaceChangeMember,
} from "../../hooks/useFiles";
import type {
  PublishWorkspaceReviewResponse,
  ReviewVerdict,
  WorkspaceReview,
} from "../../lib/types";
import { ReviewBadge, ReviewStripe } from "./ReviewBadge";
import {
  draftPublishFields,
  draftSummary,
  isReviewFileExcluded,
  useReviewDrafts,
} from "./reviewDrafts";
import {
  REVIEW_PUSH_EVENT,
  previewFileEditCount,
  sourceAdvanced,
  sourceAdvancedMessage,
} from "./reviewWorkspace";
import {
  liveGateKind,
  liveGateReason,
  GatedControl,
  useWorkspaceLiveGate,
  type LiveGateKind,
} from "../../lib/workspaceLiveGate";

/** The copy's own VM state as a gate kind; the reviewer owns the copy. */
function reviewVmGateKind(vmStatus: string): LiveGateKind {
  return liveGateKind({
    isOwner: true,
    isBase: false,
    readOnly: false,
    freezing: false,
    vmStatus,
  });
}

export interface ReviewWorkspaceBannerProps {
  review: WorkspaceReview;
  /** Only the reviewer (the review workspace owner) can push or update. */
  canPush: boolean;
  /** Seeds the review prompt into a fresh chat composer (owner only). */
  onSeedReviewPrompt?: () => void;
  /**
   * Finish review (`g5.governance.review-workspace.finish`): stops the copy
   * and offers to remove it. Offered to the owner when the copy is removable.
   */
  onFinishReview?: () => void;
  onPushed: (result: PublishWorkspaceReviewResponse) => void;
  /** Called after Update from the source finished (success or failure). */
  onUpdated?: (outcome: { ok: boolean; message: string }) => void;
  /** Opens (or focuses) the copy's Review tab. */
  onOpenReviewTab?: () => void;
  /** Opens the source workspace, the one this copy reviews. */
  onBackToSource?: () => void;
  /**
   * The mobile layout renders no center panel, so the Review tab's Push
   * review is out of reach there: the banner carries one of its own.
   */
  mobilePush?: boolean;
}

/**
 * What an Update's per-member report may carry beyond the shared member
 * shape: git's own words on a failure, the paths it could not merge, and
 * the paths where the source's version was kept over the copy's commit.
 */
type UpdateMemberExtra = Pick<
  WorkspaceChangeMember,
  "detail" | "conflicts" | "resolved"
>;

const DETAIL_LIMIT = 300;

/** What an Update's error code means to the reviewer, in a sentence. */
export function updateErrorSentence(code: string, sourceName: string): string {
  switch (code) {
    case "UPDATE_FAILED":
      return `Could not bring ${sourceName}'s latest into your copy`;
    case "DIRTY_WORKING_TREE":
      return "Your copy has unsaved changes";
    case "TRANSPORT_UNAVAILABLE":
    case "STATUS_UNAVAILABLE":
      return "Your copy is not reachable right now";
    default:
      return code;
  }
}

function updateDetail(member: UpdateMemberExtra): string | null {
  const detail = member.detail?.trim();
  if (!detail) return null;
  return detail.length > DETAIL_LIMIT
    ? `${detail.slice(0, DETAIL_LIMIT)}…`
    : detail;
}

export const ReviewWorkspaceBanner: React.FC<ReviewWorkspaceBannerProps> = ({
  review,
  canPush,
  onSeedReviewPrompt,
  onFinishReview,
  onPushed,
  onUpdated,
  onOpenReviewTab,
  onBackToSource,
  mobilePush = false,
}) => {
  const [pushOpen, setPushOpen] = useState(false);
  // Update and the agent review run in the copy's VM; Push review does not
  // (a stopped copy pushes what it saved). The reviewer reads the copy's
  // state in the review's own words (`g5.workspace.ui.vm-state`).
  // The page's gate reads the VM status the strip and composer use; the
  // review context is a separate, slower read, so it decides only outside a
  // page (`g5.workspace.ui.vm-state`).
  const gate = useWorkspaceLiveGate();
  const copyGateKind = gate ? gate.kind : reviewVmGateKind(review.vmStatus);
  const copyRunning = copyGateKind === "live";
  const copyStoppedReason = (verb: string): string | undefined =>
    copyRunning
      ? undefined
      : (liveGateReason(copyGateKind, verb, {
          workspaceNoun: "your review workspace",
        }) ?? undefined);
  // The Review tab's Push review button (the only one) opens the dialog,
  // which lives here so it works from any page of the copy.
  useEffect(() => {
    if (!canPush) return;
    const open = () => setPushOpen(true);
    window.addEventListener(REVIEW_PUSH_EVENT, open);
    return () => window.removeEventListener(REVIEW_PUSH_EVENT, open);
  }, [canPush]);
  const update = useWorkspaceChangesAction(review.reviewWorkspaceId);
  // The latest preview (My review keeps it fresh; the push dialog refetches
  // it) names where the copy stands against the source's branch. Read from
  // the shared query, never started from here.
  const preview = useReviewPreview(review.reviewWorkspaceId, false);
  // The source's current tips come with its rounds and refresh on every
  // review notification, so the warning appears while the copy stays open.
  const sourcePublications = useReviewPublications(review.sourceWorkspaceId);
  const sourceHeads =
    sourcePublications.status === "ok"
      ? sourcePublications.data.sourceHeads
      : undefined;
  const advanced =
    canPush &&
    preview.status === "ok" &&
    sourceAdvanced(preview.data, sourceHeads);

  const handleUpdate = async () => {
    try {
      const report = await update.mutateAsync({ action: "update" });
      // The action answers 200 with a per-member report: a dirty tree, a
      // fetch failure or a rebase conflict shows up as `status: "failed"`
      // or member errors, not as an HTTP error.
      const memberErrors = report.members.flatMap((member) => {
        const errors = member.errors ?? [];
        if (errors.length === 0) return [];
        const name = member.root_path || member.repo_id;
        const extra: UpdateMemberExtra = member;
        const conflicts = (extra.conflicts ?? []).filter(Boolean);
        const detail = updateDetail(extra);
        // "<repo>: Could not bring W's latest into your copy (could not
        // merge: a, b) — <git's words>": a code alone never says what stood
        // in the way.
        const sentences = errors
          .map((code) => updateErrorSentence(code, review.sourceWorkspaceName))
          .join(", ");
        return [
          [
            `${name}: ${sentences}`,
            conflicts.length > 0
              ? ` (could not merge: ${conflicts.join(", ")})`
              : "",
            detail ? ` — ${detail}` : "",
          ].join(""),
        ];
      });
      if (report.status === "failed" || memberErrors.length > 0) {
        onUpdated?.({
          ok: false,
          message:
            memberErrors.length > 0
              ? memberErrors.join("; ")
              : "The update was refused; the review continues on its previous base.",
        });
        return;
      }
      const resolved = report.members.flatMap((member) => {
        const paths = (member.resolved ?? []).filter(Boolean);
        const prefix = member.root_path ? `${member.root_path}/` : "";
        return paths.map((path) => `${prefix}${path}`);
      });
      onUpdated?.({
        ok: true,
        message:
          resolved.length > 0
            ? `Picked up ${review.sourceWorkspaceName}'s latest; ${review.sourceWorkspaceName}'s version kept in: ${resolved.join(", ")}.`
            : `This copy now sits on top of the latest changes from ${review.sourceWorkspaceName}.`,
      });
    } catch (error) {
      onUpdated?.({
        ok: false,
        message:
          error instanceof Error
            ? error.message
            : "The update was refused; the review continues on its previous base.",
      });
    }
  };

  return (
    <div data-testid="workspace-review-banner" role="status">
      <ReviewStripe />
      <div
        className={classNames(
          "flex flex-wrap items-center gap-2 px-4 py-1.5",
          "border-b border-[var(--g5-active)]/40 bg-[var(--g5-active)]/10",
          "text-xs text-[var(--g5-text1)]",
        )}
      >
        {onBackToSource && (
          <Button
            kind="ghost"
            className="!px-2 !py-1 !text-xs"
            data-testid="workspace-review-back-to-source"
            title={`Open ${review.sourceWorkspaceName}, the workspace this copy reviews`}
            onClick={onBackToSource}
          >
            <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" />
            Back to {review.sourceWorkspaceName}
          </Button>
        )}
        <ReviewBadge />
        <span className="min-w-0 flex-1">
          {canPush
            ? "Your copy of "
            : `${review.reviewerDisplayName}'s copy of `}
          <span className="font-semibold text-[var(--g5-text0)]">
            {review.sourceWorkspaceName}
          </span>
          {canPush
            ? `. Edit freely here; Push review sends your committed changes to ${review.sourceWorkspaceName} as suggestions its owner accepts one by one.`
            : `; only they can push it as a review.`}
        </span>
        {onOpenReviewTab && (
          <Button
            kind="ghost"
            className="!px-2.5 !py-1 !text-xs"
            data-testid="workspace-review-open-tab"
            title="Your changes, notes and rounds, and the source's reviews"
            onClick={onOpenReviewTab}
          >
            Review
          </Button>
        )}
        {canPush && onSeedReviewPrompt && (
          <GatedControl
            verb="run the agent review"
            reason={copyStoppedReason("run the agent review") ?? null}
            testId="workspace-review-run-hint"
          >
            <Button
              kind="ghost"
              className="!px-2.5 !py-1 !text-xs"
              data-testid="workspace-review-seed-prompt"
              onClick={onSeedReviewPrompt}
              disabled={!copyRunning}
              title={copyStoppedReason("run the agent review")}
            >
              Run agent review
            </Button>
          </GatedControl>
        )}
        {canPush && (
          <GatedControl
            verb="update"
            reason={copyStoppedReason("update") ?? null}
            testId="workspace-review-start-hint"
          >
            <Button
              kind="secondary"
              className="!px-2.5 !py-1 !text-xs"
              data-testid="workspace-review-update"
              onClick={handleUpdate}
              disabled={update.isLoading || !copyRunning}
              title={copyStoppedReason("update")}
            >
              {update.isLoading ? (
                <>
                  <Loader2
                    className="h-3.5 w-3.5 animate-spin"
                    aria-hidden="true"
                  />
                  Updating…
                </>
              ) : (
                `Update from ${review.sourceWorkspaceName}`
              )}
            </Button>
          </GatedControl>
        )}
        {canPush && mobilePush && (
          <Button
            kind="primary"
            className="!px-2.5 !py-1 !text-xs"
            data-testid="workspace-review-push-mobile"
            onClick={() => setPushOpen(true)}
          >
            Push review
          </Button>
        )}
        {canPush && onFinishReview && (
          <Button
            kind="ghost"
            className="!px-2.5 !py-1 !text-xs"
            data-testid="workspace-review-finish"
            title="Stops your review workspace and offers to remove it. Rounds you pushed stay on the source."
            onClick={onFinishReview}
          >
            Finish review
          </Button>
        )}
      </div>
      {advanced && (
        <div
          role="status"
          data-testid="review-source-advanced"
          className={classNames(
            "flex gap-2 border-b border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-4 py-1.5",
            "text-xs text-[var(--g5-text0)]",
          )}
        >
          <AlertTriangle
            className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--g5-warn)]"
            aria-hidden="true"
          />
          <span>{sourceAdvancedMessage(review.sourceWorkspaceName)}</span>
        </div>
      )}
      {pushOpen && (
        <PushReviewDialog
          review={review}
          onClose={() => setPushOpen(false)}
          onPushed={(result) => {
            setPushOpen(false);
            onPushed(result);
          }}
        />
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// PushReviewDialog
// ---------------------------------------------------------------------------

export interface PushReviewDialogProps {
  review: WorkspaceReview;
  onClose: () => void;
  onPushed: (result: PublishWorkspaceReviewResponse) => void;
}

const VERDICTS: { value: ReviewVerdict; label: string; hint: string }[] = [
  {
    value: "comment_only",
    label: "Comment only",
    hint: "Suggestions and notes; no verdict on merging.",
  },
  {
    value: "approve",
    label: "Approve",
    hint: "Good to merge as it stands.",
  },
  {
    value: "request_changes",
    label: "Request changes",
    hint: "Needs another round before it should merge.",
  },
];

export const PushReviewDialog: React.FC<PushReviewDialogProps> = ({
  review,
  onClose,
  onPushed,
}) => {
  const titleId = useId();
  const summaryId = useId();
  const verdictId = useId();
  const summaryRef = useRef<HTMLTextAreaElement | null>(null);
  const [summary, setSummary] = useState("");
  const [verdict, setVerdict] = useState<ReviewVerdict>("comment_only");
  // Opening the dialog saves the reviewer's current work in their running
  // review workspace (commit and push, with no git step on their part) and
  // previews the round from it; Push review saves again and publishes.
  // Push waits for this dialog's own preview: it is the proof the save went
  // through and the list of what the push sends. A preview My review
  // started earlier (even one still in flight, which the shared query would
  // otherwise dedupe against) may have saved the tree before the reviewer's
  // latest edits, so the dialog cancels it and requests its own; only that
  // request's result makes the push available. A failure is shown with a
  // retry instead of being skipped over.
  const preview = useReviewPreview(review.reviewWorkspaceId);
  const refreshPreview = preview.refresh;
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    let live = true;
    void refreshPreview().then((data) => {
      if (live && data) setFresh(true);
    });
    return () => {
      live = false;
    };
  }, [refreshPreview]);
  const retryPreview = () => {
    setFresh(false);
    void refreshPreview().then((data) => {
      if (data) setFresh(true);
    });
  };
  const [stale, setStale] = useState(false);
  const previewFresh = preview.status === "ok" && fresh && !preview.fetching;
  const sourcePublications = useReviewPublications(review.sourceWorkspaceId);
  const advanced =
    preview.status === "ok" &&
    sourceAdvanced(
      preview.data,
      sourcePublications.status === "ok"
        ? sourcePublications.data.sourceHeads
        : undefined,
    );
  const publish = usePublishWorkspaceReview(review.reviewWorkspaceId);
  // What the reviewer wrote in My review: notes per change and the
  // changes or files held back. Sent with the push, then cleared.
  const drafts = useReviewDrafts(review.reviewWorkspaceId);
  const reconcileDrafts = drafts.reconcile;
  // Pruning is against the dialog's own fresh preview only: a cached one
  // may predate notes another tab wrote for newer changes, and a prune
  // against it would delete them for good.
  const previewData =
    preview.status === "ok" && fresh && !preview.fetching ? preview.data : null;
  useEffect(() => {
    if (previewData) reconcileDrafts(previewData);
  }, [previewData, reconcileDrafts]);
  const draftCounts =
    preview.status === "ok" ? draftSummary(preview.data, drafts.drafts) : null;
  const excludedChanges = new Set(drafts.drafts.excludedChanges);
  const previewFailed = preview.status === "error";
  const previewPending = !previewFailed && !previewFresh;
  const workspaceStopped =
    preview.status === "ok" && preview.data.saved?.workspace === "stopped";

  const handlePush = async () => {
    setStale(false);
    try {
      const result = await publish.mutateAsync({
        summary: summary.trim() || undefined,
        verdict,
        ...draftPublishFields(
          drafts.drafts,
          preview.status === "ok" ? preview.data : undefined,
        ),
      });
      drafts.clear();
      onPushed(result);
    } catch (error) {
      // The mutation error renders inline; nothing was published. A key
      // the round no longer has means the copy changed under the list:
      // the preview is taken again here, and the push tried once more.
      const message = error instanceof Error ? error.message : "";
      if (/push review again/i.test(message)) {
        setStale(true);
        retryPreview();
      }
    }
  };

  return (
    <Modal
      labelledBy={titleId}
      onClose={onClose}
      size="sm"
      closeDisabled={publish.isLoading}
      initialFocusRef={summaryRef}
      className="p-0"
      testId="push-review-dialog"
    >
      <ReviewStripe />
      <div className="space-y-4 p-5">
        <div>
          <h2
            id={titleId}
            className="text-base font-semibold text-[var(--g5-text0)]"
          >
            Push review to {review.sourceWorkspaceName}
          </h2>
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            {review.reviewWorkspaceName}
          </p>
        </div>
        {workspaceStopped && (
          <div
            data-testid="push-review-workspace-stopped"
            role="alert"
            className={classNames(
              "flex gap-2 rounded-[var(--g5-radius-sm)] border px-3 py-2 text-xs",
              "border-[var(--g5-warn)]/60 bg-[var(--g5-warn)]/10 text-[var(--g5-text0)]",
            )}
          >
            <AlertTriangle
              className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--g5-warn)]"
              aria-hidden="true"
            />
            <div className="min-w-0">
              <p className="font-medium">Your review workspace is stopped.</p>
              <p className="mt-0.5 text-[var(--g5-text1)]">
                What it saved before stopping is what this push sends. Start it
                to include your latest edits.
              </p>
            </div>
          </div>
        )}
        {previewFailed && (
          <div
            data-testid="push-review-preview-error"
            className="flex flex-wrap items-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/50 px-3 py-2 text-xs text-[var(--g5-blocked)]"
          >
            <span>
              Your changes could not be saved and previewed:{" "}
              {preview.error.message}. Push review sends only what this preview
              shows.
            </span>
            <Button
              kind="secondary"
              className="!px-2.5 !py-1 !text-xs"
              data-testid="push-review-preview-retry"
              onClick={retryPreview}
            >
              Retry preview
            </Button>
          </div>
        )}
        {advanced && (
          <div
            data-testid="push-review-source-advanced"
            role="status"
            className={classNames(
              "flex gap-2 rounded-[var(--g5-radius-sm)] border px-3 py-2 text-xs",
              "border-[var(--g5-warn)]/60 bg-[var(--g5-warn)]/10 text-[var(--g5-text0)]",
            )}
          >
            <AlertTriangle
              className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--g5-warn)]"
              aria-hidden="true"
            />
            <span>{sourceAdvancedMessage(review.sourceWorkspaceName)}</span>
          </div>
        )}
        {preview.status === "ok" && (
          <div
            data-testid="push-review-preview"
            className="space-y-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-2 text-xs"
          >
            <p className="font-medium text-[var(--g5-text0)]">
              {preview.data.files.length === 0
                ? "No changes to publish yet."
                : `${preview.data.files.length} changed file${preview.data.files.length === 1 ? "" : "s"}`}
            </p>
            <ul className="space-y-0.5">
              {preview.data.files.map((file) => (
                <li
                  key={`${file.repoId}:${file.path}`}
                  className="flex flex-wrap items-center gap-2"
                >
                  <span className="min-w-0 flex-1 truncate font-mono text-[var(--g5-text1)]">
                    {file.path}
                  </span>
                  <span
                    className="text-[var(--g5-text2)]"
                    data-testid={`push-review-file-kind-${file.path}`}
                  >
                    {file.kind === "hunk"
                      ? `${previewFileEditCount(file)} change${previewFileEditCount(file) === 1 ? "" : "s"}`
                      : file.kind === "new_file"
                        ? "new file · the owner decides"
                        : "artifact"}
                    {isReviewFileExcluded(drafts.drafts, file.repoId, file.path)
                      ? " · left out"
                      : (() => {
                          const held = (file.changes ?? []).filter((change) =>
                            excludedChanges.has(change.key),
                          ).length;
                          return held > 0 ? ` · ${held} left out` : "";
                        })()}
                  </span>
                </li>
              ))}
            </ul>
            {draftCounts && draftCounts.total > 0 && (
              <p
                className="text-[var(--g5-text2)]"
                data-testid="push-review-drafts"
              >
                {draftCounts.included} of {draftCounts.total} change
                {draftCounts.total === 1 ? "" : "s"} included,{" "}
                {draftCounts.notes} note{draftCounts.notes === 1 ? "" : "s"}{" "}
                written in My review
              </p>
            )}
            {preview.data.skipped.length > 0 && (
              <p className="text-[var(--g5-text2)]">
                Skipped:{" "}
                {preview.data.skipped
                  .map((item) => `${item.path} (${item.reason})`)
                  .join(", ")}
              </p>
            )}
          </div>
        )}
        <ol className="space-y-2 text-sm text-[var(--g5-text1)]">
          <li>
            <span className="font-medium text-[var(--g5-text0)]">
              Changes become suggestions.
            </span>{" "}
            Every changed file in this copy is compared with{" "}
            {review.sourceWorkspaceName} and shown there as suggestions its
            owner can accept, edit or reject. Nothing is written until the owner
            accepts.
          </li>
          <li>
            <span className="font-medium text-[var(--g5-text0)]">
              Rounds stack.
            </span>{" "}
            Pushing again replaces your open suggestions with the new state;
            accepted and rejected ones keep their history.
          </li>
          <li>
            <span className="font-medium text-[var(--g5-text0)]">
              This copy stays yours.
            </span>{" "}
            Keep reviewing here; Update from {review.sourceWorkspaceName} brings
            in what the owner changed.
          </li>
        </ol>
        <div className="space-y-1">
          <label
            htmlFor={verdictId}
            className="text-xs font-medium text-[var(--g5-text1)]"
          >
            Verdict
          </label>
          <select
            id={verdictId}
            value={verdict}
            onChange={(event) =>
              setVerdict(event.target.value as ReviewVerdict)
            }
            disabled={publish.isLoading}
            data-testid="push-review-verdict"
            className={classNames(
              "w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
              "bg-[var(--g5-bg0)] px-2.5 py-2 text-sm text-[var(--g5-text0)]",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            )}
          >
            {VERDICTS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          <p className="text-xs text-[var(--g5-text2)]">
            {VERDICTS.find((option) => option.value === verdict)?.hint}
          </p>
          <p className="text-xs text-[var(--g5-text2)]">
            This verdict is feedback. Submit formal reviews for each requested
            type in the source workspace’s Approvals view.
          </p>
        </div>
        <div className="space-y-1">
          <label
            htmlFor={summaryId}
            className="text-xs font-medium text-[var(--g5-text1)]"
          >
            Summary for the owner
          </label>
          <textarea
            id={summaryId}
            ref={summaryRef}
            rows={3}
            value={summary}
            onChange={(event) => setSummary(event.target.value)}
            disabled={publish.isLoading}
            className={classNames(
              "w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
              "bg-[var(--g5-bg0)] px-2.5 py-2 text-sm text-[var(--g5-text0)]",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
            )}
            placeholder="What you changed and what still needs attention."
          />
        </div>
        {publish.error && (
          <p
            data-testid="push-review-error"
            className="text-xs text-[var(--g5-blocked)]"
          >
            {stale
              ? "Your changes moved; the list was refreshed, push again."
              : publish.error.message}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button
            kind="secondary"
            onClick={onClose}
            disabled={publish.isLoading}
            data-testid="push-review-cancel"
          >
            Keep reviewing
          </Button>
          <Button
            kind="primary"
            onClick={handlePush}
            disabled={publish.isLoading || previewPending || previewFailed}
            data-testid="push-review-confirm"
          >
            {publish.isLoading ? (
              <>
                <Loader2
                  className="h-3.5 w-3.5 animate-spin"
                  aria-hidden="true"
                />
                Pushing…
              </>
            ) : previewPending ? (
              <>
                <Loader2
                  className="h-3.5 w-3.5 animate-spin"
                  aria-hidden="true"
                />
                Saving your changes…
              </>
            ) : (
              "Push review"
            )}
          </Button>
        </div>
      </div>
    </Modal>
  );
};
