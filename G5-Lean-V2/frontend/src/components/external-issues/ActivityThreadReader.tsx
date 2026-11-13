import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { formatDateTime } from "../../lib/dateTime";
import type { ReviewComment } from "../../lib/types";
import { WorkspaceThread } from "../workspace-review/WorkspaceDiscussion";
import { ReviewDialog } from "../workspace-review/ReviewDialog";

/** Resolve the canonical parent independently of the loaded Activity pages. */
export function ActivityThreadReader({
  workspaceId,
  commentId,
  reply = false,
  onClose,
}: {
  workspaceId: string;
  commentId: string;
  reply?: boolean;
  onClose: () => void;
}) {
  // The existing comments endpoint lists workspace/line/approval parents;
  // Activity pagination must never determine whether a thread is reachable.
  const comments = useQuery({
    queryKey: ["workspace-review-comments", workspaceId, "activity-reader"],
    queryFn: ({ signal }) =>
      api.get<ReviewComment[]>(`/api/v1/workspaces/${workspaceId}/comments`, {
        signal,
      }),
  });
  const comment = comments.data?.find((row) => row.id === commentId);
  return (
    <ReviewDialog title="Review thread" onClose={onClose}>
      {comments.isPending ? (
        <p role="status">Loading thread…</p>
      ) : comment ? (
        <div data-testid="activity-thread-reader">
          <p className="mb-3 text-xs text-[var(--g5-text2)]">
            {comment.authorName} · {formatDateTime(comment.createdAt)}
          </p>
          <WorkspaceThread
            key={comment.id}
            comment={comment}
            initialReplyOpen={reply}
          />
        </div>
      ) : (
        <p role="status">
          Thread unavailable.{" "}
          <button
            type="button"
            data-testid="activity-thread-retry"
            className="text-[var(--g5-info)] hover:underline"
            onClick={() => void comments.refetch()}
          >
            Retry
          </button>
        </p>
      )}
    </ReviewDialog>
  );
}
