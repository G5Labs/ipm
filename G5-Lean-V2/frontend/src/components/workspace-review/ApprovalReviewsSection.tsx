import { ContextualHelp } from "../ui/ContextualHelp";
import { useAuth } from "../../hooks/useAuth";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import type { ReviewComment } from "../../lib/types";
import { formatApprovalDateTime } from "../../lib/dateTime";
import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import {
  useReviewPublications,
  useReviewSuggestions,
  useWorkspaceReviewContext,
} from "../../hooks/useWorkspaceReview";
import {
  ReviewsPanel,
  type ReviewsPanelProps,
} from "../workspace/ReviewsPanel";
import { ReviewWorkspaceCard } from "../workspace/ReviewWorkspaceCard";
import {
  WorkspaceFeedbackButton,
  WorkspaceThread,
} from "./WorkspaceDiscussion";

export function ApprovalReviewsSection(
  props: ReviewsPanelProps & { orgSlug: string },
) {
  const { hasScope } = useAuth();
  const location = useLocation();
  const query = new URLSearchParams(location.search);
  const deepLinked =
    query.get("view") === "reviews" ||
    !!query.get("review") ||
    !!query.get("suggestion") ||
    !!query.get("comment") ||
    /^#review-/.test(location.hash);
  const [open, setOpen] = useState(deepLinked);
  const [arrival, setArrival] = useState(location.key);
  if (arrival !== location.key) {
    setArrival(location.key);
    if (deepLinked) setOpen(true);
  }
  const publications = useReviewPublications(props.workspaceId);
  const suggestions = useReviewSuggestions(props.workspaceId);
  const context = useWorkspaceReviewContext(props.workspaceId);
  const comments = useQuery({
    queryKey: ["workspace-review-comments", props.workspaceId, "workspace"],
    queryFn: ({ signal }) =>
      api.get<ReviewComment[]>(
        `/api/v1/workspaces/${props.workspaceId}/comments`,
        { signal },
      ),
    refetchInterval: 5_000,
    enabled: open,
  });
  const commentId = query.get("comment");
  const scrolled = useRef<string | null>(null);
  useEffect(() => {
    if (!open || !commentId || scrolled.current === location.key) return;
    const target = document.getElementById(`comment-${commentId}`);
    if (target) {
      target.scrollIntoView?.({ block: "center" });
      scrolled.current = location.key;
    }
  }, [open, commentId, comments.data, location.key]);
  const openItemCount =
    suggestions.status === "ok"
      ? suggestions.data.suggestions.filter(
          (suggestion) => suggestion.status === "open",
        ).length
      : 0;
  const publicationCount =
    publications.status === "ok" ? publications.data.publications.length : 0;
  const counts =
    publications.status === "ok" && suggestions.status === "ok"
      ? `${openItemCount} open review item${openItemCount === 1 ? "" : "s"} · ${publicationCount} published review${publicationCount === 1 ? "" : "s"}`
      : "Counts unavailable";
  return (
    <>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-1 items-center gap-1">
          <h2 className="min-w-0 text-xl font-semibold text-[var(--g5-text0)]">
            <button
              type="button"
              data-testid="approvals-reviews-toggle"
              aria-expanded={open}
              aria-controls="approvals-reviews-content"
              className="inline-flex flex-wrap items-baseline gap-x-1.5 text-left"
              onClick={() => setOpen((v) => !v)}
            >
              <span aria-hidden="true">{open ? "▾" : "▸"}</span>
              <span>Reviews</span>
              <span className="text-sm font-normal text-[var(--g5-text2)]">
                · {counts}
              </span>
            </button>
          </h2>
          <ContextualHelp label="About reviews and comments">
            <p>
              <strong>Published reviews</strong> are shared review submissions,
              including suggested edits and comments on docs/files. A review can
              contain several items, or only an overall summary or verdict.
            </p>
            <p>
              <strong>Open review items</strong> are individual entries awaiting
              a decision or resolution. A published review can have zero open
              items, including after all its items are resolved.
            </p>
            <p>
              <strong>Workspace comments</strong> are general feedback entered
              here. For feedback about a specific passage or file, comment on
              that doc or file.
            </p>
          </ContextualHelp>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <WorkspaceFeedbackButton
            workspaceId={props.workspaceId}
            onOpen={() => setOpen(true)}
          />
          {!props.viewerOwnsWorkspace &&
            hasScope("workspaces:write") &&
            context.status === "ok" &&
            !context.data.asReview && (
              <ReviewWorkspaceCard
                workspaceId={props.workspaceId}
                workspaceName={props.workspaceName}
                orgSlug={props.orgSlug}
                compact
              />
            )}
        </div>
      </div>
      <div id="approvals-reviews-content" hidden={!open}>
        <ReviewsPanel
          {...props}
          showCounts={false}
          revealTarget={
            open
              ? (query.get("suggestion") ??
                query.get("review") ??
                location.hash.replace(/^#/, ""))
              : undefined
          }
        />
        <div className="mb-3 mt-6 flex flex-wrap items-center justify-between gap-3">
          <h3 className="text-sm font-semibold">Workspace comments</h3>
        </div>
        {comments.error && (
          <p role="status" className="my-3 text-xs">
            Could not refresh workspace comments.{" "}
            <button
              data-testid="approvals-feedback-retry"
              className="text-[var(--g5-info)]"
              onClick={() => void comments.refetch()}
            >
              Retry comments
            </button>
          </p>
        )}
        {comments.data
          ?.filter((c) => c.scope === "workspace")
          .map((c) => (
            <article
              key={c.id}
              id={`comment-${c.id}`}
              className="mb-4 rounded border border-[var(--g5-stroke)] p-3"
            >
              <p className="mb-2 text-xs text-[var(--g5-text2)]">
                {c.authorName} · {formatApprovalDateTime(c.createdAt)}
              </p>
              <WorkspaceThread comment={c} />
            </article>
          ))}
      </div>
    </>
  );
}
