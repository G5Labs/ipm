// =============================================================================
// Issue view — center tab and issue page
// =============================================================================
// Live read of one issue in a configured issue domain (GitHub, or the native
// G5 tracker): title, state, assignees, labels, body, comments (oldest
// first), and the workspaces linked to it. Body and comments render through
// MarkdownRenderer with `trustedContent` taken from the issue's domain, so
// an untrusted domain gets restricted Markdown (no diagrams, widgets, images
// or g5:// links). On an issue's own page (`issuePage`), members with write
// permission also comment, edit their own comments, close or reopen it, edit
// it and change its labels — a G5 issue in G5, a GitHub issue as their
// connected GitHub account, gated by its write target (g5.issue.native.page);
// viewers read. In a workspace tab the issue is read-only and links to its
// page. The view's state (open editors, outcomes) belongs to the issue it
// was opened for and is dropped when the route names another. A G5 issue's
// images load through the authenticated client, and its page lists its
// attachments (g5.issue.native.attachments).
// =============================================================================

import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { EXTERNAL_ISSUES_OFF_MESSAGE } from "../../lib/issueCommentErrors";
import { Button } from "../ui/UI";
import { Link } from "react-router-dom";
import { FileText, ShieldAlert, Tags } from "lucide-react";

import {
  MarkdownRenderer,
  type MarkdownRendererProps,
} from "../shared/MarkdownRenderer";
import { stripHtmlComments } from "../../lib/githubMarkdown";
import { useAuth } from "../../hooks/useAuth";
import {
  useExternalIssueDetail,
  useExternalIssuesFeature,
  useIssueWriteTarget,
  useRouteOrgMembership,
} from "../../hooks/useExternalIssues";
import {
  canWriteNow,
  isNativeIssue,
  isOwnComment,
  issuePagePath,
  readyWriteTarget,
  type IssuePageWriter,
} from "../../lib/issuePage";
import {
  describeFeatureCheckError,
  describeIssueRequestError,
} from "../../lib/externalIssueErrors";
import { formatDateTime } from "../../lib/dateTime";
import type {
  ExternalIssueCommentDTO,
  ExternalIssueDetailDTO,
  IssueEditDTO,
  IssueLabelChangeDTO,
  IssueWriteTargetDTO,
} from "../../lib/types";
import {
  IssueAssignees,
  IssueErrorNotice,
  IssueLabels,
  IssueStateChip,
  OpenIssueLink,
} from "./ExternalIssueParts";
import { issueUserLabel } from "../../lib/nativeIssues";
import { IssuePageCommentComposer } from "./ActivityCommentComposer";
import {
  NativeIssueAttachments,
  useNativeAttachmentImages,
} from "./NativeAttachments";
import {
  IssueEditForm,
  IssueEditPartialNotice,
  IssueLabelChangeNotice,
  IssueLabelsEditor,
  IssuePageCommentEditor,
  IssueStateButton,
  IssueWriteTargetNotice,
} from "./IssuePageControls";

export interface ExternalIssueDetailPanelProps {
  actions?: {
    id: string;
    label: string;
    onSelect: () => void;
    disabled?: boolean;
    title?: string;
  }[];
  onRequestApproval?: () => void;
  repositoryId: string;
  number: number;
  /** Tab label (the displayKey) shown while the issue loads. */
  name: string;
  /**
   * The issue's own page: offer its write controls (to members who can
   * write) and no link to itself.
   */
  issuePage?: boolean;
}

function IssueActions({
  actions,
  onRequestApproval,
}: Pick<ExternalIssueDetailPanelProps, "actions" | "onRequestApproval">) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  useEffect(() => {
    if (!open) return;
    const outside = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);
  return (
    <div ref={root} className="relative flex items-center gap-2">
      {actions?.length ? (
        <>
          <button
            ref={trigger}
            data-testid="external-issue-action-menu"
            type="button"
            aria-haspopup="menu"
            aria-expanded={open}
            aria-controls={menuId}
            onClick={() => setOpen(!open)}
            className="rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2.5 py-1 text-xs text-[var(--g5-text1)]"
          >
            Action ▾
          </button>
          {open && (
            <div
              id={menuId}
              role="menu"
              aria-label="Issue actions"
              className="absolute left-0 top-full z-20 mt-1 min-w-56 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-1 shadow-lg"
            >
              {actions.map((action) => (
                <button
                  key={action.id}
                  data-testid={`external-issue-action-${action.id}`}
                  type="button"
                  role="menuitem"
                  disabled={action.disabled}
                  title={action.title}
                  onClick={() => {
                    setOpen(false);
                    action.onSelect();
                  }}
                  className="flex w-full rounded px-2 py-1.5 text-left text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {action.label}
                </button>
              ))}
            </div>
          )}
        </>
      ) : null}
      {onRequestApproval && (
        <Button kind="secondary" onClick={onRequestApproval}>
          Request approval
        </Button>
      )}
    </div>
  );
}

type RenderImage = NonNullable<MarkdownRendererProps["renderImage"]>;

function CommentItem({
  comment,
  trusted,
  issue,
  writer,
  renderImage,
}: {
  comment: ExternalIssueCommentDTO;
  trusted: boolean;
  issue: ExternalIssueDetailDTO;
  writer: IssuePageWriter | null;
  /** A G5 issue's attachment images. */
  renderImage?: RenderImage;
}) {
  const [editing, setEditing] = useState(false);
  const [editTarget, setEditTarget] = useState<IssueWriteTargetDTO | null>(
    null,
  );
  // Authors edit their own comments: a G5 user's on a native issue, the
  // connected GitHub account's on GitHub.
  const own = writer !== null && isOwnComment(writer, comment);
  return (
    <li
      data-testid={`external-issue-comment-${comment.id}`}
      className="rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
    >
      <div className="flex flex-wrap items-center gap-2 border-b border-[var(--g5-stroke)] px-3.5 py-2 text-xs text-[var(--g5-text2)]">
        <span className="font-medium text-[var(--g5-text0)]">
          {issueUserLabel(comment.author) ?? "Unknown author"}
        </span>
        <a
          href={comment.url}
          target={writer ? undefined : "_blank"}
          rel="noopener noreferrer"
          className="hover:underline"
        >
          <time dateTime={comment.createdAt}>
            {formatDateTime(comment.createdAt)}
          </time>
        </a>
        {comment.updatedAt !== comment.createdAt && writer && (
          <span>· edited</span>
        )}
        {own && !editing && (
          <>
            <span className="flex-1" />
            <button
              type="button"
              data-testid={`issue-page-comment-edit-${comment.id}`}
              onClick={() => {
                setEditTarget(readyWriteTarget(writer));
                setEditing(true);
              }}
              className="text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
            >
              Edit
            </button>
          </>
        )}
      </div>
      <div className="px-3.5 py-2.5">
        {editing && writer ? (
          <IssuePageCommentEditor
            key={comment.updatedAt}
            orgId={writer.orgId}
            issue={issue}
            comment={comment}
            target={editTarget}
            onDone={() => setEditing(false)}
            onReload={writer.onReload}
          />
        ) : comment.body.trim() ? (
          <MarkdownRenderer
            content={stripHtmlComments(comment.body)}
            trustedContent={trusted}
            renderImage={renderImage}
          />
        ) : (
          <p className="text-sm italic text-[var(--g5-text2)]">No content.</p>
        )}
      </div>
    </li>
  );
}

function IssueDetail({
  issue,
  orgId,
  orgSlug,
  orgAdmin,
  actions,
  onRequestApproval,
  onIssuePage,
  writer,
}: Pick<ExternalIssueDetailPanelProps, "actions" | "onRequestApproval"> & {
  issue: ExternalIssueDetailDTO;
  orgId: string;
  orgSlug: string;
  /** An organization administrator removes anyone's attachments. */
  orgAdmin: boolean;
  /** Shown on the issue's own page: no link to itself. */
  onIssuePage: boolean;
  /** Page controls for a member who can write; `null` otherwise. */
  writer: IssuePageWriter | null;
}) {
  const trusted = issue.contentTrusted;
  const native = isNativeIssue(issue);
  const [editing, setEditing] = useState(false);
  // The GitHub write target as it was when editing began: a later read
  // never switches the open form's review step or account.
  const [editTarget, setEditTarget] = useState<IssueWriteTargetDTO | null>(
    null,
  );
  // A GitHub edit applied without every assignee change, until the next edit.
  const [partialEdit, setPartialEdit] = useState<IssueEditDTO | null>(null);
  const [editingLabels, setEditingLabels] = useState(false);
  const [labelTarget, setLabelTarget] = useState<IssueWriteTargetDTO | null>(
    null,
  );
  // A label change that did not fully apply, until the next one.
  const [labelOutcome, setLabelOutcome] = useState<IssueLabelChangeDTO | null>(
    null,
  );
  const writable = writer !== null && canWriteNow(writer);
  const attachmentImages = useNativeAttachmentImages(issue.canonicalUrl);
  const renderImage = native ? attachmentImages : undefined;
  return (
    <article
      data-testid="external-issue-detail"
      aria-labelledby="external-issue-title"
    >
      <header className="mb-5">
        <div className="flex flex-wrap items-center gap-2">
          <span
            data-testid="external-issue-display-key"
            className="font-mono text-xs text-[var(--g5-text2)]"
          >
            {issue.displayKey}
          </span>
          <IssueStateChip state={issue.state} stateReason={issue.stateReason} />
          <span className="flex-1" />
          {writer && !editing && (
            <>
              <Button
                kind="secondary"
                data-testid="issue-page-edit-button"
                disabled={!writable}
                onClick={() => {
                  setPartialEdit(null);
                  setEditTarget(readyWriteTarget(writer));
                  setEditing(true);
                }}
              >
                Edit
              </Button>
              <IssueStateButton
                orgId={writer.orgId}
                issue={issue}
                disabled={!writable}
              />
            </>
          )}
          {!native && !onIssuePage && (
            <Link
              to={issuePagePath(
                orgSlug,
                issue.locator.repositoryId,
                issue.locator.number,
              )}
              data-testid="external-issue-open-page"
              className="inline-flex flex-none items-center gap-1 rounded border border-[var(--g5-stroke)] px-2.5 py-1 text-xs font-medium text-[var(--g5-text0)] no-underline hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
            >
              <FileText aria-hidden="true" className="h-3.5 w-3.5" />
              Open issue page
            </Link>
          )}
          {!(native && onIssuePage) && (
            <OpenIssueLink native={native} href={issue.canonicalUrl} />
          )}
        </div>
        {writer?.target && !editing && (
          <IssueWriteTargetNotice target={writer.target} />
        )}
        {partialEdit && !editing && (
          <IssueEditPartialNotice result={partialEdit} />
        )}
        {editing && writer ? (
          <div className="mt-3">
            <IssueEditForm
              // A native edit is bound to the version read; a GitHub form
              // keeps what it started from as later reads arrive.
              key={native ? issue.updatedAt : "github"}
              orgId={writer.orgId}
              issue={issue}
              target={editTarget}
              onDone={(result) => {
                setEditing(false);
                setPartialEdit(result ?? null);
              }}
              onReload={writer.onReload}
            />
          </div>
        ) : (
          <div className="flex flex-wrap items-center gap-3">
            <h1
              id="external-issue-title"
              className="mt-2 text-xl font-semibold text-[var(--g5-text0)]"
            >
              {issue.title}
            </h1>
            {(actions?.length || onRequestApproval) && (
              <IssueActions
                actions={actions}
                onRequestApproval={onRequestApproval}
              />
            )}
          </div>
        )}
        <p className="mt-1 text-xs text-[var(--g5-text2)]">
          {issueUserLabel(issue.author) ?? "Unknown author"} opened{" "}
          <time dateTime={issue.createdAt}>
            {formatDateTime(issue.createdAt)}
          </time>
          {issue.closedAt && (
            <>
              {" "}
              · closed{" "}
              <time dateTime={issue.closedAt}>
                {formatDateTime(issue.closedAt)}
              </time>
            </>
          )}
        </p>
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <IssueAssignees assignees={issue.assignees} labelled />
          {!editingLabels && <IssueLabels labels={issue.labels} />}
          {writer && !editingLabels && !editing && (
            <Button
              kind="ghost"
              data-testid="issue-page-labels-edit"
              disabled={!writable}
              title={
                writable
                  ? undefined
                  : "Labels cannot be changed now; see why above."
              }
              onClick={() => {
                setLabelOutcome(null);
                setLabelTarget(readyWriteTarget(writer));
                setEditingLabels(true);
              }}
              className="px-2 py-0.5 text-xs"
            >
              <Tags aria-hidden="true" className="h-3.5 w-3.5" />
              {issue.labels.length > 0 ? "Edit labels" : "Add labels"}
            </Button>
          )}
        </div>
        {labelOutcome && !editingLabels && (
          <IssueLabelChangeNotice result={labelOutcome} />
        )}
        {editingLabels && writer && (
          <div className="mt-3">
            <IssueLabelsEditor
              key={`${issue.domainId}:${issue.locator.repositoryId}:${issue.locator.number}`}
              orgId={writer.orgId}
              issue={issue}
              target={labelTarget}
              onDone={(result) => {
                setEditingLabels(false);
                setLabelOutcome(result ?? null);
              }}
            />
          </div>
        )}
      </header>

      {!trusted && (
        <div
          data-testid="external-issue-restricted-notice"
          className="mb-4 flex items-start gap-2 rounded-lg border border-[rgb(242,162,73,0.3)] bg-[rgb(242,162,73,0.06)] px-3.5 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]"
        >
          <ShieldAlert
            aria-hidden="true"
            className="mt-0.5 h-4 w-4 shrink-0 text-[var(--g5-warn)]"
          />
          <p>
            {native
              ? "This issue tracker's content is not marked trusted."
              : "This repository's content is not marked trusted."}{" "}
            Diagrams, widgets, images and G5 links in the issue are shown as
            plain text.
          </p>
        </div>
      )}

      <section
        data-testid="external-issue-body"
        aria-label="Description"
        hidden={editing}
        className="rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-4 py-3"
      >
        {issue.body?.trim() ? (
          <MarkdownRenderer
            content={stripHtmlComments(issue.body)}
            trustedContent={trusted}
            renderImage={renderImage}
          />
        ) : (
          <p className="text-sm italic text-[var(--g5-text2)]">
            No description provided.
          </p>
        )}
      </section>

      {native && onIssuePage && (
        <NativeIssueAttachments
          orgId={orgId}
          number={issue.locator.number}
          removerId={writer?.userId ?? null}
          administrator={orgAdmin}
        />
      )}

      <section
        data-testid="external-issue-links"
        aria-labelledby="external-issue-links-title"
        className="mt-6"
      >
        <h2
          id="external-issue-links-title"
          className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--g5-text1)]"
        >
          Linked workspaces
        </h2>
        {issue.links.length === 0 ? (
          <p className="mt-2 text-sm text-[var(--g5-text2)]">
            No workspace is linked to this issue.
          </p>
        ) : (
          <ul className="mt-2 space-y-1.5">
            {issue.links.map((link, index) => (
              <li
                key={`${link.workspaceId ?? `hidden-${index}`}:${link.role}`}
                data-testid={`external-issue-link-${link.workspaceId ?? `hidden-${index}`}`}
                className="flex items-center gap-2 text-sm text-[var(--g5-text0)]"
              >
                <span className="w-16 flex-none text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                  {link.role === "primary" ? "Primary" : "Related"}
                </span>
                {link.workspaceName !== null ? (
                  <Link
                    to={`/${orgSlug}/${encodeURIComponent(link.workspaceName)}?view=issues`}
                    className="truncate text-[var(--g5-info)] hover:underline"
                  >
                    {link.workspaceName}
                  </Link>
                ) : (
                  <span className="text-[var(--g5-text2)]">
                    A workspace you cannot access
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section
        data-testid="external-issue-comments"
        aria-labelledby="external-issue-comments-title"
        className="mt-6"
      >
        <h2
          id="external-issue-comments-title"
          className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--g5-text1)]"
        >
          Comments ({issue.commentCount})
        </h2>
        {issue.comments.length === 0 ? (
          <p className="mt-2 text-sm text-[var(--g5-text2)]">No comments.</p>
        ) : (
          <ol className="mt-2 space-y-3">
            {issue.comments.map((comment) => (
              <CommentItem
                key={comment.id}
                comment={comment}
                trusted={trusted}
                issue={issue}
                writer={writer}
                renderImage={renderImage}
              />
            ))}
          </ol>
        )}
        {issue.commentsTruncated && (
          <p
            data-testid="external-issue-comments-truncated"
            className="mt-3 text-xs text-[var(--g5-text2)]"
          >
            Showing the first {issue.comments.length} of {issue.commentCount}{" "}
            comments.
            {!native && (
              <>
                {" "}
                <a
                  href={issue.canonicalUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[var(--g5-info)] underline"
                >
                  Read the rest in GitHub
                </a>
                .
              </>
            )}
          </p>
        )}
        {writer && <PageCommentComposer issue={issue} writer={writer} />}
      </section>
    </article>
  );
}

/**
 * The page's comment composer: a native issue's always ready; a GitHub
 * issue's once its write target is read, in that target's state.
 */
function PageCommentComposer({
  issue,
  writer,
}: {
  issue: ExternalIssueDetailDTO;
  writer: IssuePageWriter;
}) {
  const destination = {
    trackerId: issue.trackerId,
    externalIssueId: issue.externalIssueId,
  };
  if (writer.target === null) {
    return (
      <IssuePageCommentComposer
        orgId={writer.orgId}
        locator={issue.locator}
        imageUploads={{ orgId: writer.orgId, number: issue.locator.number }}
        composer={{
          provider: "g5",
          state: "ready",
          githubLogin: null,
          destination,
          displayKey: issue.displayKey,
        }}
      />
    );
  }
  if (writer.target.status !== "ok") return null;
  const target = writer.target.data;
  return (
    <IssuePageCommentComposer
      orgId={writer.orgId}
      locator={issue.locator}
      composer={{
        provider: "github",
        state: target.state,
        githubLogin: target.githubLogin,
        destination,
        displayKey: issue.displayKey,
      }}
      trusted={target.contentTrusted}
    />
  );
}

export function ExternalIssueDetailPanel({
  actions,
  onRequestApproval,
  repositoryId,
  number,
  name,
  issuePage = false,
}: ExternalIssueDetailPanelProps) {
  const membership = useRouteOrgMembership();
  const { user } = useAuth();
  const feature = useExternalIssuesFeature(membership?.orgId);
  const featureEnabled = feature.status === "enabled";
  const detail = useExternalIssueDetail(
    membership?.orgId ?? "",
    repositoryId,
    number,
    featureEnabled,
  );
  // Members with write permission change issues on their page; viewers read
  // (the server refuses their writes too).
  const canWrite =
    issuePage && !!user && !!membership && membership.role !== "viewer";
  const native = isNativeIssue({ repositoryId });
  const writeTarget = useIssueWriteTarget(
    membership?.orgId ?? "",
    { repositoryId, number },
    featureEnabled && canWrite && !native && number > 0,
  );

  // Membership comes first: without it the feature reads as disabled, which
  // would misreport a non-member as a deployment without the feature.
  let content: ReactNode;
  if (!membership) {
    content = (
      <p
        data-testid="external-issue-no-org"
        className="text-sm text-[var(--g5-text1)]"
      >
        You are not a member of this organization.
      </p>
    );
  } else if (feature.status === "disabled") {
    content = (
      <p
        data-testid="external-issue-unavailable"
        className="text-sm text-[var(--g5-text1)]"
      >
        {feature.switchedOff
          ? EXTERNAL_ISSUES_OFF_MESSAGE
          : "Issues are not available on this G5 deployment."}
      </p>
    );
  } else if (feature.status === "error") {
    content = (
      <IssueErrorNotice
        testId="external-issue-feature-error"
        presentation={describeFeatureCheckError(feature.error)}
        onRetry={feature.retry}
      />
    );
  } else if (detail.status === "error") {
    content = (
      <>
        <h1 className="mb-3 font-mono text-sm text-[var(--g5-text2)]">
          {name}
        </h1>
        <IssueErrorNotice
          testId="external-issue-error"
          presentation={describeIssueRequestError(detail.error)}
          onRetry={detail.refetch}
        />
      </>
    );
  } else if (detail.status === "ok") {
    const writer: IssuePageWriter | null =
      canWrite && user
        ? {
            orgId: membership.orgId,
            userId: user.id,
            onReload: detail.refetch,
            target: isNativeIssue(detail.data) ? null : writeTarget,
          }
        : null;
    content = (
      <IssueDetail
        // Editors and outcomes belong to the issue they were opened for.
        key={`${repositoryId}:${number}`}
        issue={detail.data}
        orgId={membership.orgId}
        orgSlug={membership.orgSlug}
        orgAdmin={membership.role === "admin"}
        actions={actions}
        onRequestApproval={onRequestApproval}
        onIssuePage={issuePage}
        writer={writer}
      />
    );
  } else {
    content = (
      <p
        role="status"
        data-testid="external-issue-loading"
        className="text-sm text-[var(--g5-text2)]"
      >
        {native ? `Loading ${name}…` : `Loading ${name} from GitHub…`}
      </p>
    );
  }

  return (
    <div
      data-testid="external-issue-panel"
      className="min-w-0 w-full px-6 py-8"
    >
      {content}
    </div>
  );
}
