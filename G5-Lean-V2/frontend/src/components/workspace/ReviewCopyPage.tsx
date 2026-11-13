/**
 * ReviewCopyPage — the reviewer's page in a review copy, as a center tab:
 * what this copy changed against the source (notes, holds, the last round
 * with the owner's decisions) followed by the source's reviews and their
 * rounds. The banner above the editor stays one line; this is where the
 * detail lives.
 */
import React from "react";
import { useParams } from "react-router-dom";
import { Loader2 } from "lucide-react";
import { useWorkspaceReviewContext } from "../../hooks/useWorkspaceReview";
import { MyReviewPanel } from "./MyReviewPanel";
import { ReviewRoundsOverview } from "./ReviewRoundsOverview";

export interface ReviewCopyPageProps {
  /** The review copy this tab belongs to. */
  workspaceId: string;
  viewerUserId: string;
  /** Only the reviewer (the copy's owner) previews and pushes. */
  canPush: boolean;
  /** Opens a file of this copy in the editor, at a line. */
  onOpenFile?: (filePath: string, repoRef?: string, line?: number) => void;
}

export const ReviewCopyPage: React.FC<ReviewCopyPageProps> = ({
  workspaceId,
  viewerUserId,
  canPush,
  onOpenFile,
}) => {
  const { org } = useParams<{ org: string }>();
  const context = useWorkspaceReviewContext(workspaceId);
  const review = context.status === "ok" ? context.data.asReview : null;

  if (context.status === "loading") {
    return (
      <div
        data-testid="review-copy-page-loading"
        className="flex items-center justify-center gap-2 p-10 text-sm text-[var(--g5-text2)]"
      >
        <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
        Loading your review…
      </div>
    );
  }
  if (!review) {
    return (
      <div
        data-testid="review-copy-page-empty"
        className="p-10 text-center text-sm text-[var(--g5-text2)]"
      >
        This workspace is not a review copy.
      </div>
    );
  }
  return (
    <div
      data-testid="review-copy-page"
      className="mx-auto w-full max-w-[960px] space-y-10 px-6 py-6"
    >
      <MyReviewPanel
        review={review}
        viewerUserId={viewerUserId}
        org={org ?? ""}
        canPush={canPush}
        onOpenFile={onOpenFile}
      />
      <ReviewRoundsOverview
        sourceWorkspaceId={review.sourceWorkspaceId}
        sourceWorkspaceName={review.sourceWorkspaceName}
        viewerUserId={viewerUserId}
      />
    </div>
  );
};
