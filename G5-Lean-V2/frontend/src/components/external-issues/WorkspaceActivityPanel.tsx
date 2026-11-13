import {
  changedRepositories,
  useWorkspaceReviewFlow,
} from "../../hooks/useWorkspaceReviewFlow";
import { WorkspaceThread } from "../workspace-review/WorkspaceDiscussion";
// =============================================================================
// Workspace Activity — center tab (`?view=activity`)
// =============================================================================
// The primary issue's discussion (comments and events, from GitHub or the
// native G5 tracker) newest first by
// last activity, interleaved with labelled workspace context: chats, tasks,
// previews, approval decisions and pull-request events — every row a durable
// record (ui g5.workbench.landing). Ongoing conversations and tasks (ui
// g5.ui.activity-current) are the first rows, badged Current, outside the
// filters and never repeated below. A filter row above the feed chooses which
// groups are shown; every kind stays loaded and the choice is per-person
// browser state (`lib/activityFilters.ts`). New workspace comments are entered
// in Reviews, GitHub issue comments in Issues. Older entries load below.
// =============================================================================

import { useEffect, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateAndReadAgain } from "../../lib/readAgain";
import { Link, useParams } from "react-router-dom";
import {
  Bot,
  CheckCircle2,
  CircleDot,
  Eye,
  ExternalLink,
  GitCommitHorizontal,
  GitPullRequest,
  ListChecks,
  MessageSquare,
  PlayCircle,
} from "lucide-react";

import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import { stripHtmlComments } from "../../lib/githubMarkdown";
import { useAuth } from "../../hooks/useAuth";
import {
  ACTIVITY_FILTER_GROUPS,
  activityFilterGroupOf,
  isShownActivityEntry,
  type ShownActivityEntry,
  readHiddenActivityGroups,
  writeHiddenActivityGroups,
  toggleActivityGroup,
  type ActivityFilterGroupId,
} from "../../lib/activityFilters";
import {
  previewEndingSentence,
  previewRunHasLog,
  previewRunTitle,
  previewStageWord,
} from "../../lib/previewRunWords";
import {
  useRouteOrgMembership,
  useWorkspaceActivity,
  useWorkspaceActivityCurrent,
} from "../../hooks/useExternalIssues";
import { formatDateTime } from "../../lib/dateTime";
import {
  describeIssueError,
  describeIssueRequestError,
} from "../../lib/externalIssueErrors";
import type {
  ActivityEntryDTO,
  CommentComposerDTO,
  PreviewOpenRequest,
  TaskStatus,
  WorkspaceActivityDTO,
} from "../../lib/types";
import { journeyStepById } from "../../lib/journeySteps";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import { ActivityCommentComposer } from "./ActivityCommentComposer";
import { ActivityCommentEditor } from "./ActivityCommentEditor";
import { IssueErrorNotice } from "./ExternalIssueParts";
import { ActivityThreadReader } from "./ActivityThreadReader";
import { useNativeAttachmentImages } from "./NativeAttachments";
import { issueUserName } from "../../lib/nativeIssues";
import { nativeIssueHref } from "../../lib/issuePage";
import { ReviewDialog } from "../workspace-review/ReviewDialog";

export interface WorkspaceActivityPanelProps {
  workspaceId: string;
  /** Open a commit's diff. */
  onOpenCommit: (commit: ActivityCommitEntry) => void;
  /**
   * Open the Pull Requests center tab; a `pull_request` row without a
   * provider URL offers it. Omitted → the row carries no link.
   */
  onOpenPullRequests?: () => void;
  /**
   * Open the preview dialog from a preview task row (ui
   * g5.ui.activity.preview-row): with its run log when the run has one.
   * Omitted → the row carries no link.
   */
  onOpenPreview?: (request: PreviewOpenRequest) => void;
  /**
   * Rendered above the feed inside its scroll container: the workbench's
   * `TimelineStatusHeader` under the alignment layout (g5.workbench.landing).
   */
  header?: ReactNode;
}

/**
 * The label of a pull-request event. An override names its actor and reason
 * and a merge on GitHub its repository (g5.workspace.merge.admin-override.formal,
 * g5.git.provider-merged-outside), so those take the entry's actor and message.
 * A failure of a PR with no GitHub records names no GitHub publication
 * (g5.git.pr-no-github), so that one takes the entry's repository count.
 */
export function pullRequestEventLabel(
  eventType: string,
  context?: {
    actorName?: string | null;
    message?: string | null;
    repositoryCount?: number | null;
  },
) {
  const message = context?.message?.trim();
  switch (eventType) {
    case "merge_overridden":
      return `Merge with override by ${context?.actorName?.trim() || "Someone"}${
        message ? `: ${message}` : ""
      }`;
    case "provider_merged_outside":
      return `Merged on GitHub outside G5${message ? `: ${message}` : ""}`;
    case "created":
      return "Pull request opened";
    case "source_rebased":
      return "Source rebased";
    case "merge_blocked":
      return "Merge blocked";
    case "merged":
      return "Merged";
    case "closed":
      return "Closed";
    case "provider_created":
      return "Published to GitHub";
    case "provider_merged":
      return "Merged on GitHub";
    case "provider_action_required":
      return "GitHub needs action";
    case "provider_failed":
      return context?.repositoryCount === 0
        ? "PR update failed"
        : "GitHub publication failed";
    case "auto_merge_enabled":
      return "Merge when ready turned on";
    case "auto_merge_disabled":
      return "Merge when ready turned off";
    default:
      return statusWord(eventType);
  }
}

type PullRequestEntry = Extract<ActivityEntryDTO, { kind: "pull_request" }>;

/** Events whose label already carries the entry's message. */
function pullRequestLabelCarriesMessage(eventType: string): boolean {
  return (
    eventType === "merge_overridden" || eventType === "provider_merged_outside"
  );
}

/**
 * "{label}: {PR title}"; a label that carries its own message is followed by
 * the title after a separator, so the reason is never read as the title.
 */
export function pullRequestEventTitle(entry: PullRequestEntry): string {
  if (!pullRequestLabelCarriesMessage(entry.eventType)) {
    return `${pullRequestEventLabel(entry.eventType, {
      repositoryCount: entry.repositoryCount,
    })}: ${entry.title}`;
  }
  const label = pullRequestEventLabel(entry.eventType, {
    actorName: entry.actor?.displayName,
    message: entry.message,
  });
  return `${label} · ${entry.title}`;
}

export type ActivityCommitEntry = Extract<ActivityEntryDTO, { kind: "commit" }>;
type IssueCommentEntry = Extract<ActivityEntryDTO, { kind: "issue_comment" }>;

function issueReplyReference(comment: IssueCommentEntry): string {
  const author = (issueUserName(comment.author) ?? "comment").replace(
    /[[\]\\]/g,
    "\\$&",
  );
  const href =
    comment.source === "g5"
      ? new URL(nativeIssueHref(comment.url), window.location.origin).href
      : comment.url;
  const markdownHref = href
    .replace(/\(/g, "%28")
    .replace(/\)/g, "%29")
    .replace(/[<>\s]/g, encodeURIComponent);
  return `> [Reply to ${author}](${markdownHref})\n\n`;
}

function eventSentence(
  entry: Extract<ActivityEntryDTO, { kind: "issue_event" }>,
) {
  const actor = issueUserName(entry.actor) ?? "Someone";
  const detail = entry.detail ?? "";
  switch (entry.event) {
    case "labeled":
      return `${actor} added the label ${detail}`;
    case "unlabeled":
      return `${actor} removed the label ${detail}`;
    case "assigned":
      return `${actor} assigned ${detail}`;
    case "unassigned":
      return `${actor} unassigned ${detail}`;
    case "closed":
      return entry.stateReason === "not_planned"
        ? `${actor} closed the issue as not planned`
        : `${actor} closed the issue`;
    case "reopened":
      return `${actor} reopened the issue`;
    case "renamed":
      return `${actor} renamed the issue to “${detail}”`;
    case "milestoned":
      return `${actor} added the milestone ${detail}`;
    case "demilestoned":
      return `${actor} removed the milestone ${detail}`;
    case "cross-referenced":
      return `Referenced from ${detail || "another issue"}`;
    case "locked":
      return `${actor} locked the conversation`;
    case "unlocked":
      return `${actor} unlocked the conversation`;
    default:
      return `${actor}: ${entry.event}`;
  }
}

function statusWord(status: string) {
  return status.replace(/_/g, " ");
}

/**
 * The task status behind an ongoing entry (ui g5.ui.activity-current): a
 * conversation's agent task, a task's own. Anything the server did not name
 * as queued or waiting reads as running.
 */
export function currentTaskStatus(entry: ActivityEntryDTO): TaskStatus {
  const status =
    entry.kind === "chat"
      ? entry.taskStatus
      : entry.kind === "task"
        ? entry.status
        : undefined;
  return status === "queued" || status === "waiting" ? status : "running";
}

/**
 * The Current badge's word: a preview row's stage (ui
 * g5.ui.activity.preview-row), any other row's task state.
 */
export function currentBadgeWord(entry: ActivityEntryDTO): string {
  if (entry.kind === "task" && entry.preview?.stage) {
    return previewStageWord(entry.preview.stage, entry.preview.port);
  }
  return currentStateWord(currentTaskStatus(entry));
}

/** The Current badge's short state word. */
export function currentStateWord(status: TaskStatus) {
  switch (status) {
    case "queued":
      return "Queued";
    case "waiting":
      return "Waiting";
    default:
      return "Running";
  }
}

/**
 * "Maya Chen approved Final review": approvals are workspace-wide, so no
 * phase is named (g5.governance.approval.requirements). A withdrawn
 * (`deferred`) response reads "withdrew response for"
 * (g5.workbench.landing.approval-events).
 */
export function approvalDecisionTitle(
  entry: Extract<ActivityEntryDTO, { kind: "approval_decision" }>,
) {
  const actor =
    entry.actorName ??
    (entry.actorType === "agent" ? "Agent reviewer" : "A reviewer");
  const action =
    entry.outcome === "deferred" ? "withdrew response for" : entry.outcome;
  return `${actor} ${action} ${entry.definitionName ?? "an approval"}`;
}

/**
 * The line under an approval event's headline: a human's comment (rendered
 * by the caller as its thread when one exists) or an agent's authored
 * summary. An agent's `comment` is never shown: on older servers it held a
 * clip of the report body (g5.workbench.landing.approval-events).
 */
export function approvalDecisionSummary(
  entry: Extract<ActivityEntryDTO, { kind: "approval_decision" }>,
): string | null {
  const text =
    entry.actorType === "agent" ? entry.summary : (entry.comment ?? null);
  return text?.trim() ? text.trim() : null;
}

/**
 * Only a legacy decision recorded without a request is historical. An older
 * server without the flag marked a human decision with no PR this way; a new
 * request made before a PR exists is not historical.
 */
export function approvalDecisionHistorical(
  entry: Extract<ActivityEntryDTO, { kind: "approval_decision" }>,
): boolean {
  return (
    entry.historical ??
    (entry.actorType === "human" && entry.pullRequestId === null)
  );
}

/** A guided-journey mark by its catalog label (g5.workbench.journey.marks). */
export function journeyStepTitle(
  entry: Extract<ActivityEntryDTO, { kind: "journey_step" }>,
) {
  const step = journeyStepById(entry.stepId);
  const label = step?.label ?? entry.stepId;
  return entry.done
    ? `Marked done: ${label}`
    : entry.skipped
      ? `Skipped: ${label}`
      : `Marked not done: ${label}`;
}

/** Why a task ended, as the owner would say it; the raw word otherwise. */
const TASK_END_REASONS: Record<string, string> = {
  idle_stop: "Stopped because the workspace went idle",
  owner_stop: "Stopped with the workspace",
  owner_cancel: "Cancelled by you",
  transfer: "Stopped for an ownership transfer",
  start_required: "Stopped because the workspace had to restart",
  host_drain: "Stopped while its host was drained",
  workspace_stop: "Stopped with the workspace",
  vm_lost: "Ended because the workspace VM was lost",
};

function taskEndReason(reason: string | null | undefined) {
  if (!reason) return undefined;
  return TASK_END_REASONS[reason] ?? `Ended: ${statusWord(reason)}`;
}

interface EntryView {
  icon: ReactNode;
  title: ReactNode;
  body?: ReactNode;
  controls?: ReactNode;
  /** Rows whose body is a message a person wrote are boxed; events are plain. */
  card?: boolean;
  /** Only comments need an exact per-comment permalink on the timestamp. */
  permalink?: string;
  external?: boolean;
}

const ICON_CLASSES = "h-3.5 w-3.5";
const FOOTER_LINK_CLASSES =
  "text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";
const EVENT_LINK_CLASSES =
  "text-left hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

function pillClasses(pressed: boolean): string {
  return `inline-flex items-center rounded-full border px-2.5 py-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] ${
    pressed
      ? "border-transparent bg-[var(--g5-phase-be-soft)] text-[var(--g5-phase-be)]"
      : "border-[var(--g5-stroke)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
  }`;
}

function ActivityEntry({
  entry,
  orgSlug,
  workspaceSlug,
  contentTrusted,
  onOpenCommit,
  onOpenPullRequests,
  onOpenPreview,
  onOpenThread,
  onReplyIssue,
  primaryIssue,
  workspaceId,
  editableAs,
  viewerUserId = null,
  current = false,
}: {
  entry: ShownActivityEntry;
  orgSlug: string;
  workspaceSlug: string;
  contentTrusted: boolean;
  onOpenCommit: (commit: ActivityCommitEntry) => void;
  onOpenPullRequests?: () => void;
  /**
   * Open the preview dialog from a preview task row (ui
   * g5.ui.activity.preview-row): with its run log when the run has one.
   * Omitted → the row carries no link.
   */
  onOpenPreview?: (request: PreviewOpenRequest) => void;
  onOpenThread: (commentId: string, reply?: boolean) => void;
  onReplyIssue?: (comment: IssueCommentEntry) => void;
  primaryIssue: WorkspaceActivityDTO["primaryIssue"];
  workspaceId: string;
  editableAs: CommentComposerDTO | null;
  viewerUserId?: string | null;
  current?: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const review = useWorkspaceReviewFlow(workspaceId);
  const workspacePath = `/${orgSlug}/${encodeURIComponent(workspaceSlug)}`;
  const nativeDiscussion = entry.source === "g5";
  // A G5 issue's comments show its attachments through the reader's own
  // authenticated read (g5.issue.native.attachments).
  const attachmentImages = useNativeAttachmentImages(
    primaryIssue?.canonicalUrl,
  );
  const trackerLink = (url: string, label: ReactNode) =>
    nativeDiscussion ? (
      <Link to={nativeIssueHref(url)} className={EVENT_LINK_CLASSES}>
        {label}
      </Link>
    ) : (
      <a
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        className={EVENT_LINK_CLASSES}
      >
        {label}
      </a>
    );
  const threadLink = (commentId: string, label: string) => (
    <button
      type="button"
      data-testid={`activity-open-thread-${commentId}`}
      className={EVENT_LINK_CLASSES}
      onClick={() => onOpenThread(commentId)}
    >
      {label}
    </button>
  );
  let view: EntryView;
  switch (entry.kind) {
    case "issue_opened": {
      const identity =
        primaryIssue?.canonicalUrl === entry.url
          ? primaryIssue.displayKey
          : entry.title;
      view = {
        icon: <CircleDot aria-hidden="true" className={ICON_CLASSES} />,
        title: trackerLink(
          entry.url,
          `${issueUserName(entry.author) ?? "Someone"} opened ${identity}`,
        ),
      };
      break;
    }
    case "issue_comment": {
      const own = nativeDiscussion
        ? editableAs?.provider === "g5" &&
          !!viewerUserId &&
          entry.author?.userId === viewerUserId
        : editableAs?.githubLogin != null &&
          entry.author?.login.toLowerCase() ===
            editableAs.githubLogin.toLowerCase();
      view = {
        icon: <MessageSquare aria-hidden="true" className={ICON_CLASSES} />,
        title: `${issueUserName(entry.author) ?? "Someone"} commented`,
        body:
          editing && editableAs ? (
            <ActivityCommentEditor
              workspaceId={workspaceId}
              composer={editableAs}
              commentId={entry.commentId}
              body={entry.body}
              updatedAt={entry.updatedAt}
              trusted={contentTrusted}
              onDone={() => setEditing(false)}
            />
          ) : (
            <MarkdownRenderer
              content={stripHtmlComments(entry.body)}
              trustedContent={contentTrusted}
              renderImage={nativeDiscussion ? attachmentImages : undefined}
            />
          ),
        controls: !editing ? (
          <div className="flex gap-3">
            <button
              type="button"
              data-testid={`activity-comment-reply-${entry.commentId}`}
              disabled={!onReplyIssue}
              onClick={() => onReplyIssue?.(entry)}
              className={`${FOOTER_LINK_CLASSES} disabled:opacity-50`}
            >
              Reply
            </button>
            {own && (
              <button
                type="button"
                data-testid={`activity-comment-edit-${entry.commentId}`}
                onClick={() => setEditing(true)}
                className={FOOTER_LINK_CLASSES}
              >
                Edit
              </button>
            )}
          </div>
        ) : undefined,
        permalink: entry.url,
        card: true,
      };
      break;
    }
    case "issue_event":
      view = {
        icon: <CircleDot aria-hidden="true" className={ICON_CLASSES} />,
        title: eventSentence(entry),
      };
      break;
    case "chat":
      view = {
        icon: <Bot aria-hidden="true" className={ICON_CLASSES} />,
        title: (
          <Link
            to={`${workspacePath}?session=${encodeURIComponent(entry.sessionId)}`}
            className={EVENT_LINK_CLASSES}
          >
            {entry.title || "Chat"}
          </Link>
        ),
        body: `${entry.messageCount} ${entry.messageCount === 1 ? "message" : "messages"}`,
      };
      break;
    case "task":
      if (entry.taskKind === "preview" && entry.preview) {
        // A preview run (ui g5.ui.activity.preview-row): titled by its
        // profile with the ending verb, worded like the run log, and opening
        // the preview dialog, never the task's empty Workspace changes.
        const preview = entry.preview;
        const title = previewRunTitle(preview);
        const request: PreviewOpenRequest = {
          taskId: entry.taskId,
          log: previewRunHasLog(preview),
        };
        view = {
          icon: <Eye aria-hidden="true" className={ICON_CLASSES} />,
          title: onOpenPreview ? (
            <button
              type="button"
              data-testid={`activity-open-preview-${entry.taskId}`}
              onClick={() => onOpenPreview(request)}
              className={EVENT_LINK_CLASSES}
              title={request.log ? "Open the preview log" : "Open the preview"}
            >
              {title}
            </button>
          ) : (
            <span>{title}</span>
          ),
          body: preview.ending
            ? previewEndingSentence(preview.ending)
            : undefined,
        };
        break;
      }
      view = {
        icon:
          entry.taskKind === "preview" ? (
            <Eye aria-hidden="true" className={ICON_CLASSES} />
          ) : (
            <PlayCircle aria-hidden="true" className={ICON_CLASSES} />
          ),
        title: (
          <Link
            to={`${workspacePath}/task/${encodeURIComponent(entry.taskId)}`}
            className={EVENT_LINK_CLASSES}
          >
            {entry.name}
            {!current && ` ${statusWord(entry.status)}`}
          </Link>
        ),
        body: taskEndReason(entry.reason),
      };
      break;
    case "commit":
      view = {
        icon: (
          <GitCommitHorizontal aria-hidden="true" className={ICON_CLASSES} />
        ),
        title: (
          <button
            type="button"
            data-testid={`activity-open-commit-${entry.shortSha}`}
            className={EVENT_LINK_CLASSES}
            onClick={() => onOpenCommit(entry)}
          >
            {entry.summary || entry.shortSha}
          </button>
        ),
        body: (
          <>
            <span className="font-mono">{entry.shortSha}</span> ·{" "}
            {entry.authorName || "Unknown author"} · {entry.repoName}/
            {entry.branchName}
          </>
        ),
      };
      break;
    case "approval_decision": {
      const summary = approvalDecisionSummary(entry);
      const thread = entry.thread;
      view = {
        card: !!thread || summary !== null,
        icon: <CheckCircle2 aria-hidden="true" className={ICON_CLASSES} />,
        title: (
          <Link
            to={`${workspacePath}?view=approvals`}
            className={EVENT_LINK_CLASSES}
          >
            {approvalDecisionTitle(entry)}
          </Link>
        ),
        body: (
          <>
            {approvalDecisionHistorical(entry) && (
              <p className="mb-2 text-xs text-[var(--g5-text2)]">
                Historical approval · not linked to a PR
              </p>
            )}
            {thread ? (
              <WorkspaceThread
                comment={{
                  ...thread,
                  authorName: entry.actorName ?? "Reviewer",
                }}
                showReplies={false}
                onReply={() => onOpenThread(thread.id, true)}
              />
            ) : (
              summary !== null && (
                <p
                  data-testid={`activity-approval-summary-${entry.key}`}
                  className="break-words"
                >
                  {summary}
                </p>
              )
            )}
            {!!entry.reviewedGitHeads?.length &&
              review.data &&
              changedRepositories(entry.reviewedGitHeads, review.data.heads)
                .length > 0 && (
                <p className="mt-2 text-xs text-[var(--g5-warn)]">
                  Changed since this review in{" "}
                  {changedRepositories(
                    entry.reviewedGitHeads,
                    review.data.heads,
                  )
                    .map(
                      (id) =>
                        review.data?.repos.find((r) => r.id === id)?.name ?? id,
                    )
                    .join(", ")}
                  .
                </p>
              )}
          </>
        ),
      };
      break;
    }
    case "workspace_comment":
      view = {
        icon: <MessageSquare aria-hidden="true" className={ICON_CLASSES} />,
        title: threadLink(
          entry.comment.id,
          `${entry.comment.authorName ?? "A reader"} commented`,
        ),
        body: (
          <WorkspaceThread
            comment={entry.comment}
            showReplies={false}
            onReply={() => onOpenThread(entry.comment.id, true)}
          />
        ),
        card: true,
      };
      break;
    case "workspace_reply":
      view = {
        card: true,
        icon: <MessageSquare aria-hidden="true" className={ICON_CLASSES} />,
        title: threadLink(
          entry.commentId,
          `${entry.actorName ?? "A reader"} replied`,
        ),
        body: (
          <MarkdownRenderer content={entry.content} trustedContent={false} />
        ),
        controls: (
          <button
            type="button"
            data-testid={`activity-reply-${entry.key}`}
            className={`${FOOTER_LINK_CLASSES} disabled:opacity-50`}
            disabled={
              !review.data || !!review.error || review.data.workspace.readOnly
            }
            onClick={() => onOpenThread(entry.commentId, true)}
          >
            Reply
          </button>
        ),
      };
      break;
    case "approval_requested":
      view = {
        icon: <CheckCircle2 aria-hidden="true" className={ICON_CLASSES} />,
        title: (
          <Link
            to={`${workspacePath}?view=approvals`}
            className={EVENT_LINK_CLASSES}
          >{`${entry.actorName ?? "Workspace owner"} ${entry.cancelled ? "cancelled the request for" : "requested"} ${entry.definitionName} from ${entry.reviewerName ?? "a reviewer"}`}</Link>
        ),
        body: entry.note ? (
          <p className="whitespace-pre-wrap">{entry.note}</p>
        ) : undefined,
      };
      break;
    case "workspace_created":
      view = {
        icon: <CircleDot aria-hidden="true" className={ICON_CLASSES} />,
        title: `Workspace created${entry.actorName ? ` by ${entry.actorName}` : ""}`,
      };
      break;
    case "journey_step": {
      const step = journeyStepById(entry.stepId);
      view = {
        icon: <ListChecks aria-hidden="true" className={ICON_CLASSES} />,
        title: journeyStepTitle(entry),
        body: (
          <span data-testid={`activity-journey-step-${entry.key}`}>
            {step ? WORKBENCH_PHASES[step.phase].short : "Guided"} ·{" "}
            {entry.actor?.displayName ?? "Unknown user"}
          </span>
        ),
      };
      break;
    }
    case "pull_request": {
      // The target's timeline names where incoming work came from.
      const direction = entry.incoming
        ? entry.sourceWorkspaceName
          ? `from ${entry.sourceWorkspaceName}`
          : ""
        : entry.targetWorkspaceName
          ? `into ${entry.targetWorkspaceName}`
          : "";
      const providerRef =
        entry.providerPrNumber !== null && entry.providerRepoFullName
          ? `${entry.providerRepoFullName}#${entry.providerPrNumber}`
          : entry.repositoryCount > 1
            ? `${entry.repositoryCount} repositories`
            : null;
      const compact =
        entry.eventType === "created" &&
        review.data?.pr?.id === entry.pullRequestId;
      const title = compact
        ? `${entry.actor?.displayName ?? "Someone"} opened the pull request`
        : pullRequestEventTitle(entry);
      view = {
        icon: <GitPullRequest aria-hidden="true" className={ICON_CLASSES} />,
        title: entry.providerPrUrl ? (
          <a
            href={entry.providerPrUrl}
            target="_blank"
            rel="noopener noreferrer"
            className={EVENT_LINK_CLASSES}
          >
            {title}
          </a>
        ) : onOpenPullRequests ? (
          <button
            type="button"
            data-testid={`activity-open-pull-requests-${entry.pullRequestId}`}
            onClick={onOpenPullRequests}
            className={EVENT_LINK_CLASSES}
          >
            {title}
          </button>
        ) : (
          title
        ),
        external: !!entry.providerPrUrl,
        body: (
          <>
            {direction}
            {providerRef && (
              <>
                {direction ? " · " : ""}
                <span className={entry.providerPrUrl ? "font-mono" : ""}>
                  {providerRef}
                </span>
              </>
            )}
            {!compact && entry.actor && (
              <>
                {direction || providerRef ? " · " : ""}
                {entry.actor.displayName}
              </>
            )}
            {entry.failureCount !== null &&
              entry.failureCount > 1 &&
              entry.firstFailedAt && (
                <p className="mt-1 text-xs text-[var(--g5-text2)]">
                  Failed {entry.failureCount} times since{" "}
                  {formatDateTime(entry.firstFailedAt)}
                </p>
              )}
            {entry.message &&
              !pullRequestLabelCarriesMessage(entry.eventType) && (
                <p className="mt-1 text-xs text-[var(--g5-text2)]">
                  {entry.message}
                </p>
              )}
          </>
        ),
      };
      break;
    }
  }

  const currentState = current ? currentBadgeWord(entry) : null;
  return (
    <li
      data-testid={`activity-entry-${entry.key}`}
      data-source={entry.source}
      data-current={current ? "true" : undefined}
    >
      <article
        id={
          entry.kind === "workspace_comment"
            ? `comment-${entry.comment.id}`
            : entry.kind === "approval_decision" && entry.thread
              ? `comment-${entry.thread.id}`
              : undefined
        }
        data-comment-id={
          entry.kind === "workspace_comment"
            ? entry.comment.id
            : entry.kind === "approval_decision"
              ? entry.thread?.id
              : undefined
        }
        data-occurred-at={entry.at}
        className={`grid grid-cols-[4.5rem_1.75rem_minmax(0,1fr)] gap-x-2 py-2 ${
          current
            ? "-ml-3 border-l-2 border-[var(--g5-info)] pl-[0.625rem]"
            : ""
        }`}
      >
        <time
          dateTime={entry.at}
          className="pt-1 text-right text-[11px] text-[var(--g5-text2)]"
        >
          {view.permalink
            ? trackerLink(view.permalink, formatDateTime(entry.at))
            : formatDateTime(entry.at)}
        </time>
        <span
          className={`mt-0.5 flex h-6 w-6 items-center justify-center rounded-full border bg-[var(--g5-bg1)] ${
            current
              ? "border-[var(--g5-info)]/50 text-[var(--g5-info)]"
              : "border-[var(--g5-stroke)] text-[var(--g5-text1)]"
          }`}
        >
          {view.icon}
        </span>
        <div
          data-card={view.card ? "true" : undefined}
          className={
            view.card
              ? "min-w-0 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2.5"
              : "min-w-0 pt-0.5"
          }
        >
          <p className="flex flex-wrap items-center gap-2 text-sm font-medium text-[var(--g5-text0)]">
            <span>{view.title}</span>
            {(entry.source === "github" || view.external) && (
              <span
                title="GitHub"
                role="img"
                aria-label="Source: GitHub"
                className="text-[var(--g5-text2)]"
              >
                <ExternalLink size={12} aria-hidden="true" />
              </span>
            )}
            {currentState && (
              <span
                data-testid={`activity-current-badge-${entry.key}`}
                className={`inline-flex items-center gap-1.5 rounded px-1.5 py-px text-[10px] font-semibold uppercase tracking-wide ${
                  currentState === "Waiting" || currentState === "Closing"
                    ? "bg-[rgb(242_162_73_/_0.18)] text-[var(--g5-warn)]"
                    : currentState === "Queued"
                      ? "bg-[var(--g5-bg2)] text-[var(--g5-text1)]"
                      : "bg-[rgb(103_164_255_/_0.18)] text-[var(--g5-info)]"
                }`}
              >
                <span
                  aria-hidden="true"
                  className="h-1.5 w-1.5 rounded-full bg-current motion-safe:animate-pulse"
                />
                Current · {currentState}
              </span>
            )}
          </p>
          {view.body && (
            <div className="mt-1 text-sm text-[var(--g5-text1)]">
              {view.body}
            </div>
          )}
          {view.controls && <div className="mt-1 text-xs">{view.controls}</div>}
        </div>
      </article>
    </li>
  );
}

export function WorkspaceActivityPanel({
  workspaceId,
  onOpenCommit,
  onOpenPullRequests,
  onOpenPreview,
  header,
}: WorkspaceActivityPanelProps) {
  const membership = useRouteOrgMembership();
  const { workspace: workspaceSlug = "" } = useParams<{ workspace: string }>();
  const [thread, setThread] = useState<{ id: string; reply: boolean } | null>(
    null,
  );
  const [issueReply, setIssueReply] = useState<{
    comment: IssueCommentEntry;
    composer: CommentComposerDTO;
    trusted: boolean;
  } | null>(null);
  const [dialogWorkspaceId, setDialogWorkspaceId] = useState(workspaceId);
  if (dialogWorkspaceId !== workspaceId) {
    setDialogWorkspaceId(workspaceId);
    setThread(null);
    setIssueReply(null);
  }
  const openThread = (id: string, reply = false) => setThread({ id, reply });
  const activity = useWorkspaceActivity(workspaceId);
  const current = useWorkspaceActivityCurrent(workspaceId);
  const queryClient = useQueryClient();
  const review = useWorkspaceReviewFlow(workspaceId);
  // The small review snapshot already polls. Only a changed snapshot refreshes
  // the paged activity feed; unchanged polls must not refetch its GitHub history.
  const reviewVersion = review.data
    ? JSON.stringify([
        review.data.pr,
        review.data.requests,
        review.data.decisions,
        review.data.heads,
        review.data.workspace.phase,
      ])
    : null;
  const observedReview = useRef<{
    workspaceId: string;
    version: string;
  } | null>(null);
  useEffect(() => {
    if (reviewVersion === null) return;
    const previous = observedReview.current;
    observedReview.current = { workspaceId, version: reviewVersion };
    if (
      previous?.workspaceId === workspaceId &&
      previous.version !== reviewVersion
    ) {
      void invalidateAndReadAgain(queryClient, [
        "workspace-activity",
        workspaceId,
      ]);
    }
  }, [queryClient, workspaceId, reviewVersion]);
  const { user } = useAuth();
  const userId = user?.id ?? "";
  // Hidden groups: per-person browser state when signed in, else component
  // state only. Stored as the hidden set so every group defaults to shown.
  // The signed-in person resolves after the first render, so the stored
  // selection is (re)read when the user id changes (React's "adjust state
  // on a prop change" pattern, no effect).
  const [hidden, setHidden] = useState<ReadonlySet<ActivityFilterGroupId>>(
    () => new Set(readHiddenActivityGroups(userId)),
  );
  const [filtersUserId, setFiltersUserId] = useState(userId);
  if (filtersUserId !== userId) {
    setFiltersUserId(userId);
    setHidden(new Set(readHiddenActivityGroups(userId)));
  }
  const applyHidden = (next: ReadonlySet<ActivityFilterGroupId>) => {
    setHidden(next);
    writeHiddenActivityGroups(userId, [...next]);
  };
  const toggleGroup = (id: ActivityFilterGroupId) => {
    applyHidden(toggleActivityGroup(hidden, id));
  };
  // Newest first: older pages append below, so nothing moves on load.
  const loadEarlier = () => void activity.fetchNextPage();

  const phaseKey = `g5:timeline-phase:${workspaceId}:${user?.id}`;
  const [reviewPhase, setReviewPhase] = useState(
    () => localStorage.getItem(phaseKey) ?? "all",
  );
  const [savedPhaseKey, setSavedPhaseKey] = useState(phaseKey);
  if (savedPhaseKey !== phaseKey) {
    setSavedPhaseKey(phaseKey);
    setReviewPhase(localStorage.getItem(phaseKey) ?? "all");
  }
  const pages = activity.data?.pages ?? [];
  const latest = pages[0];
  const primary = latest?.primaryIssue ?? null;
  // Ongoing work leads the list and is never repeated in the record below
  // (ui g5.ui.activity-current). An entry can reach two pages when its time
  // changed between loads (a task completing moves it later); keep the copy
  // from the newest page. Phase changes are served but never shown
  // (`lib/activityFilters.ts`).
  const currentEntries = (current.data?.entries ?? []).filter(
    isShownActivityEntry,
  );
  const seen = new Set<string>(currentEntries.map((entry) => entry.key));
  const entries = pages.flatMap((page) =>
    page.entries.filter((entry): entry is ShownActivityEntry => {
      if (!isShownActivityEntry(entry) || seen.has(entry.key)) return false;
      seen.add(entry.key);
      return true;
    }),
  );
  const groupCounts = new Map<ActivityFilterGroupId, number>();
  for (const entry of entries) {
    const group = activityFilterGroupOf(entry.kind);
    groupCounts.set(group, (groupCounts.get(group) ?? 0) + 1);
  }
  // Client-side over the loaded pages: the API has no kind filter, so a
  // hidden kind is never re-queried.
  const visibleEntries = entries.filter(
    (entry) =>
      !hidden.has(activityFilterGroupOf(entry.kind)) &&
      (reviewPhase === "all" ||
        (entry.kind === "workspace_comment"
          ? entry.comment.phase
          : "phase" in entry
            ? entry.phase
            : null) === reviewPhase),
  );

  // Ongoing work is its own read (ui g5.ui.activity-current): its rows and
  // its failure notice show whatever the record's state is.
  const currentNotice = current.status === "error" && (
    <p
      role="status"
      data-testid="workspace-activity-current-error"
      className="mb-2 text-xs text-[var(--g5-text2)]"
    >
      Ongoing work could not be loaded.{" "}
      <button
        type="button"
        data-testid="workspace-activity-current-retry"
        onClick={() => void current.refetch()}
        className={FOOTER_LINK_CLASSES}
      >
        Retry
      </button>
    </p>
  );
  const currentRows = currentEntries.map((entry) => (
    <ActivityEntry
      key={entry.key}
      entry={entry}
      current
      orgSlug={membership?.orgSlug ?? ""}
      workspaceSlug={workspaceSlug}
      contentTrusted={primary?.contentTrusted ?? false}
      onOpenCommit={onOpenCommit}
      onOpenPullRequests={onOpenPullRequests}
      onOpenPreview={onOpenPreview}
      workspaceId={workspaceId}
      editableAs={null}
      primaryIssue={primary}
      onOpenThread={openThread}
    />
  ));
  const currentOnlyList = currentEntries.length > 0 && (
    <ol
      aria-label="Activity"
      data-testid="workspace-activity-entries"
      className="mt-4"
    >
      {currentRows}
    </ol>
  );

  let content: ReactNode;
  if (activity.status === "pending") {
    content = (
      <>
        <p
          role="status"
          data-testid="workspace-activity-loading"
          className="text-sm text-[var(--g5-text2)]"
        >
          Loading activity…
        </p>
        {currentNotice}
        {currentOnlyList}
      </>
    );
  } else if (activity.status === "error") {
    content = (
      <>
        <IssueErrorNotice
          testId="workspace-activity-error"
          presentation={describeIssueRequestError(activity.error)}
          onRetry={() => void activity.refetch()}
        />
        {currentNotice}
        {currentOnlyList}
      </>
    );
  } else {
    content = (
      <>
        {latest?.issueError && (
          <div className="mb-4">
            <IssueErrorNotice
              testId="workspace-activity-issue-error"
              presentation={describeIssueError(
                latest.issueError.issueError,
                latest.issueError.message,
              )}
              onRetry={() => void activity.refetch()}
            >
              <p className="mt-1 text-xs text-[var(--g5-text2)]">
                Workspace context is still shown below.
              </p>
            </IssueErrorNotice>
          </div>
        )}
        {pages.some((page) => page.issueHistoryTruncated) && (
          <p
            data-testid="workspace-activity-truncated"
            className="mb-3 text-xs text-[var(--g5-text2)]"
          >
            {primary?.provider === "g5"
              ? "Only the first 100 issue events are shown; open the issue page for its full history."
              : "GitHub returned only the first 100 issue events; open the issue in GitHub for its full history."}
          </p>
        )}
        {currentNotice}
        {currentEntries.length === 0 && entries.length === 0 ? (
          <p
            data-testid="workspace-activity-empty"
            className="text-sm text-[var(--g5-text2)]"
          >
            No activity yet.
          </p>
        ) : entries.length > 0 && visibleEntries.length === 0 ? (
          <>
            {currentOnlyList}
            <p
              data-testid="workspace-activity-all-filtered"
              className="text-sm text-[var(--g5-text2)]"
            >
              No matching history for these filters.
            </p>
          </>
        ) : (
          <ol aria-label="Activity" data-testid="workspace-activity-entries">
            {currentRows}
            {visibleEntries.map((entry) => (
              <ActivityEntry
                key={entry.key}
                entry={entry}
                orgSlug={membership?.orgSlug ?? ""}
                workspaceSlug={workspaceSlug}
                contentTrusted={primary?.contentTrusted ?? false}
                onOpenCommit={onOpenCommit}
                onOpenPullRequests={onOpenPullRequests}
                onOpenPreview={onOpenPreview}
                workspaceId={workspaceId}
                editableAs={
                  latest?.composer?.state === "ready" ? latest.composer : null
                }
                viewerUserId={userId || null}
                primaryIssue={primary}
                onOpenThread={openThread}
                onReplyIssue={
                  latest?.composer
                    ? (comment) =>
                        setIssueReply({
                          comment,
                          composer: latest.composer!,
                          trusted: primary?.contentTrusted ?? false,
                        })
                    : undefined
                }
              />
            ))}
          </ol>
        )}
        {activity.hasNextPage && (
          <button
            type="button"
            data-testid="workspace-activity-load-earlier"
            onClick={loadEarlier}
            disabled={activity.isFetchingNextPage}
            className="mt-3 text-sm text-[var(--g5-info)] hover:underline disabled:opacity-60"
          >
            {activity.isFetchingNextPage
              ? "Loading earlier activity…"
              : "Load earlier activity"}
          </button>
        )}
      </>
    );
  }

  return (
    <div
      data-testid="workspace-activity-panel"
      role="region"
      aria-label="Workspace timeline"
      className="flex h-full min-h-0 flex-col"
    >
      <div
        data-testid="workspace-activity-scroller"
        className="min-h-0 flex-1 overflow-y-auto"
      >
        {/* Summary and discussion share the scroll area so short windows
          never squeeze the timeline into an unusable strip. */}
        {header && (
          <div
            data-testid="workspace-activity-header"
            className="min-w-0 w-full shrink-0 px-6 pt-8"
          >
            {header}
          </div>
        )}
        <div className={`min-w-0 w-full px-6 pb-8 ${header ? "pt-2" : "pt-8"}`}>
          <div className="mb-4 flex items-center justify-between gap-3">
            <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
              Activity
            </h1>
            {activity.status === "success" && (
              <label className="ml-auto inline-flex min-w-0 items-center gap-1.5 text-xs text-[var(--g5-text1)]">
                Phase
                <select
                  aria-label="Timeline phase filter"
                  value={reviewPhase}
                  onChange={(e) => {
                    setReviewPhase(e.target.value);
                    localStorage.setItem(phaseKey, e.target.value);
                  }}
                  className="min-w-0 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-1.5 py-0.5"
                >
                  <option value="all">All phases</option>
                  {(["be", "se", "ie"] as const).map((p) => (
                    <option key={p} value={p}>
                      {WORKBENCH_PHASES[p].name}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          {activity.status === "success" && (
            <fieldset
              data-testid="activity-filters"
              className="mb-4 flex flex-wrap items-center gap-1.5 text-xs"
            >
              <legend className="sr-only">Show activity kinds</legend>
              <button
                type="button"
                data-testid="activity-filter-all"
                aria-pressed={hidden.size === 0}
                onClick={() => applyHidden(new Set())}
                className={pillClasses(hidden.size === 0)}
              >
                All
              </button>
              {ACTIVITY_FILTER_GROUPS.map((group) => {
                const count = groupCounts.get(group.id) ?? 0;
                const shown = hidden.size > 0 && !hidden.has(group.id);
                return (
                  <button
                    key={group.id}
                    type="button"
                    data-testid={`activity-filter-${group.id}`}
                    aria-pressed={shown}
                    onClick={() => toggleGroup(group.id)}
                    className={pillClasses(shown)}
                  >
                    {group.label}
                    <span
                      data-testid={`activity-filter-count-${group.id}`}
                      className="ml-1.5 tabular-nums opacity-70"
                    >
                      {count}
                    </span>
                  </button>
                );
              })}
            </fieldset>
          )}
          {content}
          {thread && (
            <ActivityThreadReader
              key={`${workspaceId}:${thread.id}:${thread.reply}`}
              workspaceId={workspaceId}
              commentId={thread.id}
              reply={thread.reply}
              onClose={() => setThread(null)}
            />
          )}
          {issueReply && (
            <ReviewDialog
              title={`Reply to ${issueUserName(issueReply.comment.author) ?? "comment"}`}
              onClose={() => setIssueReply(null)}
            >
              <ActivityCommentComposer
                workspaceId={workspaceId}
                composer={issueReply.composer}
                trusted={issueReply.trusted}
                focusInput
                initialBody={issueReplyReference(issueReply.comment)}
              />
            </ReviewDialog>
          )}
        </div>
      </div>
    </div>
  );
}
