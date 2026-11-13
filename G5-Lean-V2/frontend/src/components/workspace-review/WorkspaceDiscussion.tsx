import { useEffect, useRef, useState } from "react";
import { randomUuid } from "../../lib/randomUuid";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { api } from "../../lib/api";
import { useAuth } from "../../hooks/useAuth";
import {
  useRefreshReview,
  useWorkspaceReviewFlow,
  type ReviewComment,
} from "../../hooks/useWorkspaceReviewFlow";
import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import { ReviewDialog, reviewInput, reviewButton } from "./ReviewDialog";
export function WorkspaceThread({
  comment,
  showReplies = true,
  initialReplyOpen = false,
  onReply,
}: {
  comment: ReviewComment;
  /** Activity projects replies as separate events; the full reader keeps them. */
  showReplies?: boolean;
  initialReplyOpen?: boolean;
  /** Activity opens the full conversation before composing a reply. */
  onReply?: () => void;
}) {
  const { user } = useAuth();
  const navigate = useNavigate();
  const [replyOpen, setReplyOpen] = useState(initialReplyOpen);
  const replyInput = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (!replyOpen) return;
    const frame = requestAnimationFrame(() => replyInput.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [replyOpen]);
  const container = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(
    () => typeof IntersectionObserver === "undefined",
  );
  useEffect(() => {
    if (!container.current || typeof IntersectionObserver === "undefined")
      return;
    const observer = new IntersectionObserver(
      ([entry]) => setVisible(entry.isIntersecting),
      { rootMargin: "200px" },
    );
    observer.observe(container.current);
    return () => observer.disconnect();
  }, []);
  const refresh = useRefreshReview(comment.workspaceId);
  const overview = useWorkspaceReviewFlow(comment.workspaceId);
  const canDiscuss =
    !!overview.data && !overview.error && !overview.data.workspace.readOnly;
  const replies = useQuery({
    queryKey: ["workspace-review-replies", comment.id],
    queryFn: () =>
      api.get<{ id: string; content: string; authorName: string }[]>(
        `/api/v1/comments/${comment.id}/replies`,
      ),
    enabled: showReplies && (visible || replyOpen),
    refetchInterval: showReplies && (visible || replyOpen) ? 5000 : false,
  });
  const key = `g5:review-reply:${user?.id}:${comment.id}`;
  const [text, setText] = useState(() => sessionStorage.getItem(key) ?? "");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [messageId, setMessageId] = useState(
    () => sessionStorage.getItem(`${key}:id`) ?? randomUuid(),
  );
  return (
    <div ref={container} data-testid={`comment-thread-${comment.id}`}>
      <MarkdownRenderer content={comment.content} trustedContent={false} />
      {comment.filePath && (
        <>
          <button
            data-testid="review-discussion-open"
            type="button"
            className="text-xs text-[var(--g5-info)]"
            onClick={() =>
              navigate(
                `?view=source-control&reviewRepo=${encodeURIComponent(comment.repoId ?? "")}&reviewPath=${encodeURIComponent(comment.filePath ?? "")}${comment.commitSha ? `&comment=${comment.id}` : "&mode=all"}`,
              )
            }
          >
            {comment.filePath}:{comment.lineNumber} ·{" "}
            {comment.commitSha ? "View discussion" : "View current file"}
          </button>
          {!comment.commitSha && (
            <p className="text-xs text-[var(--g5-text2)]">
              Left on unpushed work; current content is available to the running
              workspace’s owner.
            </p>
          )}
          <span className="sr-only" data-testid="comment-thread-anchor">
            L{comment.lineNumber}
          </span>
        </>
      )}
      <div className="ml-3 border-l border-[var(--g5-stroke)] pl-3">
        {showReplies &&
          replies.data?.map((r) => (
            <div key={r.id} className="mt-3">
              <p className="text-xs text-[var(--g5-text2)]">{r.authorName}</p>
              <MarkdownRenderer content={r.content} trustedContent={false} />
            </div>
          ))}
      </div>
      {showReplies && replies.error && (
        <p role="alert">
          Replies unavailable.{" "}
          <button
            data-testid="review-replies-retry"
            onClick={() => void replies.refetch()}
          >
            Retry
          </button>
        </p>
      )}
      <button
        data-testid="review-reply-open"
        type="button"
        className="mt-3 text-xs text-[var(--g5-info)]"
        disabled={busy || !canDiscuss}
        onClick={onReply ?? (() => setReplyOpen((v) => !v))}
      >
        Reply
      </button>
      {replyOpen && (
        <form
          className="mt-3"
          onSubmit={async (e) => {
            e.preventDefault();
            if (busy || !canDiscuss || !text.trim()) return;
            setBusy(true);
            setError("");
            sessionStorage.setItem(`${key}:id`, messageId);
            try {
              await api.post(`/api/v1/comments/${comment.id}/replies`, {
                id: messageId,
                content: text,
              });
              setText("");
              sessionStorage.removeItem(key);
              sessionStorage.removeItem(`${key}:id`);
              setMessageId(randomUuid());
              await refresh();
            } catch (e) {
              setError(e instanceof Error ? e.message : "Reply failed");
            } finally {
              setBusy(false);
            }
          }}
        >
          <label className="text-xs">
            Reply
            <textarea
              ref={replyInput}
              disabled={busy}
              aria-label="Write a reply"
              className={reviewInput}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                sessionStorage.setItem(key, e.target.value);
              }}
            />
          </label>
          <button
            type="submit"
            data-testid="review-reply-submit"
            className={reviewButton}
            disabled={busy || !canDiscuss || !text.trim()}
          >
            Post reply
          </button>
          {error && <p role="alert">{error}</p>}
        </form>
      )}
    </div>
  );
}
export function WorkspaceFeedbackButton({
  workspaceId,
  onOpen,
}: {
  workspaceId: string;
  onOpen?: () => void;
}) {
  const { user } = useAuth();
  const [open, setOpen] = useState(false);
  const key = `g5:review-feedback:${user?.id}:${workspaceId}`;
  const [draft, setDraft] = useState(() => {
    try {
      return JSON.parse(sessionStorage.getItem(key) ?? "null") as {
        id: string;
        content: string;
      } | null;
    } catch {
      return null;
    }
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const refresh = useRefreshReview(workspaceId);
  const overview = useWorkspaceReviewFlow(workspaceId);
  const canDiscuss =
    !!overview.data && !overview.error && !overview.data.workspace.readOnly;
  return (
    <>
      <button
        disabled={!canDiscuss}
        data-testid="review-leave-feedback"
        type="button"
        className={reviewButton}
        onClick={() => {
          setDraft((d) => d ?? { id: randomUuid(), content: "" });
          setOpen(true);
          onOpen?.();
        }}
      >
        Add workspace comment
      </button>
      {open && draft && (
        <ReviewDialog
          busy={busy}
          title="Add workspace comment"
          onClose={() => setOpen(false)}
        >
          <form
            onSubmit={async (e) => {
              e.preventDefault();
              if (busy || !canDiscuss || !draft.content.trim()) return;
              setBusy(true);
              setError("");
              try {
                await api.post(`/api/v1/workspaces/${workspaceId}/comments`, {
                  ...draft,
                  scope: "workspace",
                });
                sessionStorage.removeItem(key);
                setDraft(null);
                setOpen(false);
                await refresh();
              } catch (e) {
                setError(e instanceof Error ? e.message : "Comment failed");
              } finally {
                setBusy(false);
              }
            }}
          >
            <label>
              Comment
              <textarea
                disabled={busy}
                aria-label="Comment"
                className={`${reviewInput} min-h-32`}
                value={draft.content}
                onChange={(e) => {
                  const next = { ...draft, content: e.target.value };
                  setDraft(next);
                  sessionStorage.setItem(key, JSON.stringify(next));
                }}
              />
            </label>
            {error && <p role="alert">{error}</p>}
            <button
              data-testid="review-feedback-submit"
              type="submit"
              className={reviewButton}
              disabled={busy || !canDiscuss || !draft.content.trim()}
            >
              Post comment
            </button>
          </form>
        </ReviewDialog>
      )}
    </>
  );
}
