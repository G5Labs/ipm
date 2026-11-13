// =============================================================================
// Workspace rail — Issues & tasks (g5.workbench.issues)
// =============================================================================
// The workspace's issue links, primary first and related ones as subtasks.
// Quick-add opens the Create issue dialog (without Create & Fork) and links
// the new issue to this workspace as related; if linking fails the issue
// still exists and Import task can link it. When the primary issue is in the
// repository new issues go to, the dialog starts with it as the parent, so
// the task is its sub-issue (g5.external-issues.creation.parent). Quick-add is disabled with the
// reason whenever creation or linking is. The Done box closes (as completed)
// or reopens the issue as the viewer's own GitHub account
// (g5.external-issues.transition), or in G5 for a native issue without any
// GitHub connection (g5.issue.native.writes); a refusal is shown under the
// row. A link
// that arrives after the list first loaded (quick-add here, a meeting
// decision's attached task, another tab) is spotlighted once in place; the
// cue follows the data, never a press, so nothing fires on first load or when
// linking failed (g5.workbench.issues.arrival).
// =============================================================================

import { useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";

import {
  WorkspaceIssuesPanel,
  type WorkspaceIssuesPanelProps,
} from "./WorkspaceIssuesPanel";
import {
  useChangeIssueState,
  useExternalIssuesFeature,
  useIssueCreateTarget,
  useLinkWorkspaceIssue,
  useRouteOrgMembership,
  useWorkspaceIssueLinks,
} from "../../hooks/useExternalIssues";
import { useGithubConnection } from "../../hooks/useGithubConnection";
import {
  describeFeatureCheckError,
  describeIssueRequestError,
} from "../../lib/externalIssueErrors";
import {
  createTargetBlockedReason,
  describeIssueStateError,
} from "../../lib/issueCommentErrors";
import type { CreatedIssueDTO, WorkspaceIssueLinkDTO } from "../../lib/types";
import { classNames } from "../ui/UI";
import { CreateIssueDialog } from "./CreateIssueDialog";
import { IssueErrorNotice, IssueStateChip } from "./ExternalIssueParts";
import {
  ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE,
  issueStateActionTitle,
  issueStateBlockedReason,
  nextIssueState,
} from "../../lib/issueState";
import { IssueLinkReadError } from "./WorkspaceIssueLinkParts";

/** Linking is a workspace-owner operation on the server. */
export const ISSUE_QUICK_ADD_OWNER_REASON =
  "Only the workspace owner can add tasks to this workspace";

export const ISSUE_QUICK_ADD_LINKS_LOADING_REASON = "Loading linked issues…";

export function WorkspaceIssuesSection({
  workspaceId,
  onOpenIssue,
  viewerOwnsWorkspace = false,
  writeBlockedReason,
  linkBlockedReason,
}: Omit<WorkspaceIssuesPanelProps, "viewerOwnsWorkspace"> & {
  viewerOwnsWorkspace?: boolean;
}) {
  const membership = useRouteOrgMembership();
  const { workspace = "" } = useParams();
  const feature = useExternalIssuesFeature(membership?.orgId);
  const links = useWorkspaceIssueLinks(
    workspaceId,
    feature.status === "enabled",
  );
  const [creating, setCreating] = useState(false);
  const [unlinked, setUnlinked] = useState<CreatedIssueDTO | null>(null);
  const linkIssue = useLinkWorkspaceIssue(workspaceId);
  // Link ids rendered so far (null until the first successful read) and the
  // ones that arrived after it, which carry the spotlight.
  const seenLinkIds = useRef<Set<string> | null>(null);
  const [arrivedLinkIds, setArrivedLinkIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const loadedLinks = links.status === "ok" ? links.data.links : null;
  useEffect(() => {
    if (!loadedLinks) return;
    const ids = loadedLinks.map((link) => link.id);
    const seen = seenLinkIds.current;
    if (seen === null) {
      seenLinkIds.current = new Set(ids);
      return;
    }
    const arrived = ids.filter((id) => !seen.has(id));
    if (arrived.length === 0) return;
    for (const id of arrived) seen.add(id);
    setArrivedLinkIds((current) => new Set([...current, ...arrived]));
  }, [loadedLinks]);
  // Done / Reopen: the viewer's own write (GitHub needs a connection), one
  // row at a time.
  const connection = useGithubConnection(feature.status === "enabled").status;
  const changeState = useChangeIssueState(workspaceId);
  const [stateChangingLinkId, setStateChangingLinkId] = useState<string | null>(
    null,
  );
  const [stateNotice, setStateNotice] = useState<{
    linkId: string;
    message: string;
  } | null>(null);
  const toggleDone = async (link: WorkspaceIssueLinkDTO) => {
    const state = nextIssueState(link);
    setStateChangingLinkId(link.id);
    setStateNotice(null);
    try {
      const result = await changeState.mutateAsync({ linkId: link.id, state });
      if (result.outcome === "unknown") {
        setStateNotice({
          linkId: link.id,
          message: ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE,
        });
      }
    } catch (error) {
      setStateNotice({
        linkId: link.id,
        message: describeIssueStateError(error, state),
      });
    } finally {
      setStateChangingLinkId(null);
    }
  };
  // Quick-add creates and then links: only the owner may link, so only the
  // owner's rail asks where new issues go.
  const linkCreationBlockedReason = writeBlockedReason || linkBlockedReason;
  const canQuickAdd = viewerOwnsWorkspace && !linkCreationBlockedReason;
  const createTarget = useIssueCreateTarget(
    membership?.orgId ?? "",
    feature.status === "enabled" && canQuickAdd,
  );
  const quickAddReason =
    linkCreationBlockedReason ||
    (viewerOwnsWorkspace ? null : ISSUE_QUICK_ADD_OWNER_REASON) ||
    createTargetBlockedReason(createTarget) ||
    // The dialog takes its parent when it opens: wait for the primary issue.
    (links.status === "loading" ? ISSUE_QUICK_ADD_LINKS_LOADING_REASON : null);
  const linkCreated = async (issue: CreatedIssueDTO) => {
    setCreating(false);
    setUnlinked(null);
    try {
      await linkIssue.mutateAsync({ ref: issue.displayKey, role: "related" });
    } catch {
      setUnlinked(issue);
    }
  };
  if (
    feature.status === "disabled" &&
    feature.switchedOff &&
    viewerOwnsWorkspace
  ) {
    return (
      <WorkspaceIssuesPanel
        workspaceId={workspaceId}
        viewerOwnsWorkspace={viewerOwnsWorkspace}
        writeBlockedReason={writeBlockedReason}
        linkBlockedReason={linkBlockedReason}
        onOpenIssue={onOpenIssue}
      />
    );
  }
  if (feature.status === "loading" || feature.status === "disabled")
    return null;
  const rows =
    links.status === "ok"
      ? [...links.data.links].sort(
          (a, b) => Number(b.role === "primary") - Number(a.role === "primary"),
        )
      : [];
  const destination =
    createTarget.status === "ok" ? createTarget.data.destination : null;
  const primary = rows.find((link) => link.role === "primary");
  const quickAddParent =
    primary &&
    destination &&
    primary.locator.repositoryId === destination.repositoryId
      ? primary.displayKey
      : undefined;
  return (
    <section
      data-testid="workspace-issues-section"
      aria-label="Issues & tasks"
      className="p-2 text-xs text-[var(--g5-text1)]"
    >
      {feature.status === "error" ? (
        <IssueErrorNotice
          testId="workspace-issues-feature-error"
          presentation={describeFeatureCheckError(feature.error)}
          onRetry={feature.retry}
        />
      ) : (
        <>
          {links.status === "loading" && (
            <p role="status">Loading linked issues…</p>
          )}
          {links.status === "error" && (
            <IssueErrorNotice
              testId="workspace-issues-links-error"
              presentation={describeIssueRequestError(links.error)}
              onRetry={links.refetch}
            />
          )}
          {links.status === "ok" && rows.length === 0 && (
            <p>No issues are linked to this workspace.</p>
          )}
          <ul
            aria-label="Linked issues"
            className="divide-y divide-[var(--g5-stroke)]"
          >
            {rows.map((link) => {
              const stateReason = issueStateBlockedReason(
                link,
                writeBlockedReason,
                connection,
              );
              const arrived = arrivedLinkIds.has(link.id);
              return (
                <li
                  key={link.id}
                  data-testid={`workspace-issues-item-${link.id}`}
                  data-arrived={arrived ? "true" : undefined}
                  className={classNames(
                    "rounded-[var(--g5-radius-sm)] py-2",
                    arrived && "g5-row-spotlight",
                  )}
                >
                  <div className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      aria-label={`Done: ${link.displayKey}`}
                      data-testid={`workspace-issues-done-${link.id}`}
                      checked={link.issue?.state === "closed"}
                      disabled={
                        stateReason !== null || stateChangingLinkId !== null
                      }
                      title={stateReason ?? issueStateActionTitle(link)}
                      onChange={() => void toggleDone(link)}
                    />
                    <button
                      type="button"
                      data-testid={`workspace-issues-row-${link.id}`}
                      onClick={() => onOpenIssue(link)}
                      className="flex min-w-0 flex-1 flex-wrap items-center gap-1 text-left hover:bg-[var(--g5-bg2)]"
                    >
                      <span className="font-mono text-[var(--g5-text2)]">
                        {link.displayKey}
                      </span>
                      <span className="rounded-full border border-[var(--g5-stroke)] px-1.5 text-[10px] uppercase">
                        {link.role === "primary" ? "Primary" : "Subtask"}
                      </span>
                      {link.issue && (
                        <IssueStateChip
                          state={link.issue.state}
                          stateReason={link.issue.stateReason}
                        />
                      )}
                      <span className="w-full truncate">
                        {link.issue?.title}
                      </span>
                    </button>
                  </div>
                  {!link.issue && <IssueLinkReadError link={link} />}
                  {stateChangingLinkId === link.id && (
                    <p role="status" className="mt-1 text-[var(--g5-text2)]">
                      {nextIssueState(link) === "closed"
                        ? "Marking as done…"
                        : "Reopening…"}
                    </p>
                  )}
                  {stateNotice?.linkId === link.id && (
                    <div
                      role="alert"
                      data-testid={`workspace-issues-state-notice-${link.id}`}
                      className="mt-1 rounded border border-[var(--g5-stroke)] p-2"
                    >
                      <p>{stateNotice.message}</p>
                      <button
                        type="button"
                        data-testid={`workspace-issues-state-notice-dismiss-${link.id}`}
                        onClick={() => setStateNotice(null)}
                        className="mt-1 text-[var(--g5-text2)] underline hover:text-[var(--g5-text0)]"
                      >
                        Dismiss
                      </button>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
          <Link
            to={`/${membership?.orgSlug ?? ""}/${encodeURIComponent(workspace)}?view=issues`}
            data-testid="workspace-issues-open-comments"
            className="mt-2 inline-block text-[var(--g5-info)] hover:underline"
          >
            Open issue comments
          </Link>
          {linkIssue.isLoading && (
            <p role="status" className="mt-2">
              Linking the new issue…
            </p>
          )}
          {unlinked && (
            <div
              role="alert"
              data-testid="workspace-issues-quick-add-link-failed"
              className="mt-2 rounded border border-[var(--g5-stroke)] p-2"
            >
              <p>
                Created{" "}
                <a
                  href={unlinked.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[var(--g5-info)] hover:underline"
                >
                  {unlinked.displayKey}
                </a>
                , but it could not be linked to this workspace. Import task can
                link it.
              </p>
              {linkIssue.error && (
                <p className="mt-1 text-[var(--g5-text2)]">
                  {describeIssueRequestError(linkIssue.error).title}
                </p>
              )}
              <button
                type="button"
                data-testid="workspace-issues-quick-add-link-failed-dismiss"
                onClick={() => {
                  setUnlinked(null);
                  linkIssue.reset();
                }}
                className="mt-1 text-[var(--g5-text2)] underline hover:text-[var(--g5-text0)]"
              >
                Dismiss
              </button>
            </div>
          )}
          <button
            type="button"
            data-testid="workspace-issues-quick-add"
            disabled={quickAddReason !== null}
            title={
              quickAddReason ??
              (quickAddParent
                ? `Create a sub-issue of ${quickAddParent} and link it to this workspace`
                : "Create an issue and link it to this workspace")
            }
            onClick={() => setCreating(true)}
            className="mt-2 w-full rounded border border-dashed border-[var(--g5-stroke)] p-2 text-left text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-[var(--g5-text2)]"
          >
            Add a task…
          </button>
          {membership && (
            <CreateIssueDialog
              orgId={membership.orgId}
              open={creating}
              onClose={() => setCreating(false)}
              onCreated={(issue) => void linkCreated(issue)}
              initialParent={quickAddParent}
            />
          )}
        </>
      )}
    </section>
  );
}
