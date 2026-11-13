// =============================================================================
// Workspace Issues — section of Workspace Details (`?view=issues` opens it)
// =============================================================================
// Lists the workspace's active issue links (GitHub or native G5, primary
// first) with each issue's live summary or its typed read error, links an
// issue found with the organization's issue search (words, or a pasted URL,
// `owner/name#N` or native `ACME-12` reference), and unlinks after
// confirmation. Unlinking never changes the issue. While the
// organization has issues off, the workspace owner still sees the links (key
// and role only, no issue content) so they can unlink them.
// =============================================================================

import { useEffect, useId, useRef, useState } from "react";
import { useLocation, useSearchParams } from "react-router-dom";
import { CircleDot, Link2, LoaderCircle, Unlink } from "lucide-react";

import { Button, classNames } from "../ui/UI";
import { useAuth } from "../../hooks/useAuth";
import {
  useExternalIssueSearch,
  useExternalIssuesFeature,
  useLinkWorkspaceIssue,
  useRouteOrgMembership,
  useUnlinkWorkspaceIssue,
  useWorkspaceActivity,
  useWorkspaceIssueLinks,
} from "../../hooks/useExternalIssues";
import { useIssueSearchText } from "../../hooks/useIssueSearchText";
import {
  describeFeatureCheckError,
  describeIssueRequestError,
  linkConflictOf,
} from "../../lib/externalIssueErrors";
import type {
  ExternalIssueSummaryDTO,
  WorkspaceIssueLinkDTO,
  WorkspaceIssueLinkRole,
} from "../../lib/types";
import {
  IssueAssignees,
  IssueErrorNotice,
  IssueLabels,
  IssueStateChip,
  OpenIssueLink,
} from "./ExternalIssueParts";
import { isNativeIssue } from "../../lib/issuePage";
import {
  IssueLinkReadError,
  IssueUnlinkConfirmation,
} from "./WorkspaceIssueLinkParts";
import { IssueLinkConflictNotice } from "./IssueLinkConflictNotice";
import { ActivityCommentComposer } from "./ActivityCommentComposer";
import { IssueSearchOutcome } from "./IssueSearchOutcome";

export interface WorkspaceIssuesPanelProps {
  workspaceId: string;
  /** Linking and unlinking are workspace-owner operations on the server. */
  viewerOwnsWorkspace: boolean;
  /**
   * Why the owner may not link or unlink here (a review copy follows its
   * source); shown in place of the form. `null` when they may.
   */
  writeBlockedReason?: string | null;
  /**
   * Why the owner may not create a link here while unlinking stays
   * possible (Base takes no issue links, g5.issue.base-links). `null` when
   * they may.
   */
  linkBlockedReason?: string | null;
  /** Open the read-only view of a linked issue. */
  onOpenIssue: (link: WorkspaceIssueLinkDTO) => void;
}

function RoleBadge({ role }: { role: WorkspaceIssueLinkRole }) {
  return (
    <span
      data-testid="workspace-issue-link-role"
      className={classNames(
        "inline-flex flex-none items-center rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide",
        role === "primary"
          ? "border-[rgb(103,164,255,0.45)] bg-[rgb(103,164,255,0.10)] text-[var(--g5-info)]"
          : "border-[var(--g5-stroke)] text-[var(--g5-text2)]",
      )}
    >
      {role === "primary" ? "Primary" : "Related"}
    </span>
  );
}

function IssueLinkRow({
  link,
  canWrite,
  writeBlockedReason = null,
  onOpenIssue,
  onUnlink,
  unlinking,
  unlinkError,
  cleanupOnly,
}: {
  link: WorkspaceIssueLinkDTO;
  /** Unlink stays visible, disabled with this reason (a review copy). */
  writeBlockedReason?: string | null;
  canWrite: boolean;
  onOpenIssue: (link: WorkspaceIssueLinkDTO) => void;
  onUnlink: (link: WorkspaceIssueLinkDTO) => Promise<boolean>;
  unlinking: boolean;
  unlinkError: unknown;
  /** External issues are off: no issue content, only unlinking. */
  cleanupOnly: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
  const issue = cleanupOnly ? null : link.issue;
  return (
    <li data-testid={`workspace-issue-link-${link.id}`} className="px-4 py-3">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <RoleBadge role={link.role} />
        <span className="flex-none font-mono text-xs text-[var(--g5-text2)]">
          {link.displayKey}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-[var(--g5-text0)]">
          {issue?.title ?? ""}
        </span>
        {issue && (
          <IssueStateChip state={issue.state} stateReason={issue.stateReason} />
        )}
      </div>
      {issue && (
        <div className="mt-1.5 flex flex-wrap items-center gap-2">
          <IssueAssignees assignees={issue.assignees} />
          <IssueLabels labels={issue.labels} />
        </div>
      )}
      {!issue && !cleanupOnly && (
        <div className="mt-2">
          <IssueLinkReadError link={link} />
        </div>
      )}
      {confirming ? (
        <IssueUnlinkConfirmation
          link={link}
          unlinking={unlinking}
          unlinkError={unlinkError}
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            void onUnlink(link).then((ok) => {
              if (ok) setConfirming(false);
            });
          }}
        />
      ) : (
        <div className="mt-2.5 flex flex-wrap items-center gap-2">
          {!cleanupOnly && (
            <>
              <Button
                kind="secondary"
                onClick={() => onOpenIssue(link)}
                data-testid="workspace-issue-open"
                aria-label={`Open ${link.displayKey}`}
                className="px-2.5 py-1 text-xs"
              >
                Open
              </Button>
              <OpenIssueLink
                native={isNativeIssue(link)}
                href={link.canonicalUrl}
              />
            </>
          )}
          {canWrite && (
            <Button
              kind="ghost"
              onClick={() => setConfirming(true)}
              data-testid="workspace-issue-unlink"
              aria-label={`Unlink ${link.displayKey}`}
              className="px-2.5 py-1 text-xs"
              disabled={writeBlockedReason !== null}
              title={writeBlockedReason ?? undefined}
            >
              <Unlink aria-hidden="true" className="h-3.5 w-3.5" />
              Unlink
            </Button>
          )}
        </div>
      )}
    </li>
  );
}

/** Anchor of the section inside Workspace Details (`?view=issues`). */
export const WORKSPACE_ISSUES_SECTION_ID = "workspace-issues";

const SECTION_CLASSES =
  "scroll-mt-4 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4";
const HEADING_CLASSES = "text-sm font-semibold text-[var(--g5-text0)]";

/** A tracker issue's identity, as links and search results carry it. */
function issueIdentity(issue: { trackerId: string; externalIssueId: string }) {
  return `${issue.trackerId}:${issue.externalIssueId}`;
}

function IssueSearchResultRow({
  issue,
  linked,
  linking,
  disabled,
  onLink,
}: {
  issue: ExternalIssueSummaryDTO;
  linked: boolean;
  linking: boolean;
  disabled: boolean;
  onLink: () => void;
}) {
  return (
    <li
      data-testid={`workspace-issue-result-${issue.displayKey}`}
      className="flex items-center gap-3 px-3 py-2"
    >
      <span className="flex-none font-mono text-xs text-[var(--g5-text2)]">
        {issue.displayKey}
      </span>
      <span className="min-w-0 flex-1 truncate text-sm text-[var(--g5-text0)]">
        {issue.title}
      </span>
      <IssueStateChip state={issue.state} stateReason={issue.stateReason} />
      {linked ? (
        <span className="flex-none text-xs text-[var(--g5-text2)]">Linked</span>
      ) : (
        <Button
          kind="secondary"
          type="button"
          disabled={disabled}
          onClick={onLink}
          data-testid="workspace-issue-result-link"
          aria-label={`Link ${issue.displayKey}`}
        >
          {linking ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Link2 aria-hidden="true" className="h-3.5 w-3.5" />
          )}
          Link
        </Button>
      )}
    </li>
  );
}

export function WorkspaceIssuesPanel({
  workspaceId,
  viewerOwnsWorkspace,
  writeBlockedReason = null,
  linkBlockedReason = null,
  onOpenIssue,
}: WorkspaceIssuesPanelProps) {
  const refId = useId();
  const roleName = useId();
  const headingId = useId();
  const { hasScope } = useAuth();
  const membership = useRouteOrgMembership();
  const orgSlug = membership?.orgSlug ?? "";
  const feature = useExternalIssuesFeature(membership?.orgId);
  const featureEnabled = feature.status === "enabled";
  // The owner keeps the controls; a review copy shows them disabled with
  // the reason (`g5.governance.review-workspace.exclusions`).
  const canWrite = viewerOwnsWorkspace && hasScope("workspaces:write");
  const writeBlocked = canWrite && writeBlockedReason !== null;
  // The link form's own block: every write, or only new links (Base).
  const linkFormBlockedReason = writeBlockedReason ?? linkBlockedReason;
  const linkFormBlocked = canWrite && linkFormBlockedReason !== null;
  // Off for the organization: the owner can still unlink existing links.
  const cleanupOnly =
    feature.status === "disabled" && feature.switchedOff === true && canWrite;
  const links = useWorkspaceIssueLinks(
    workspaceId,
    featureEnabled || cleanupOnly,
  );
  const linkIssue = useLinkWorkspaceIssue(workspaceId);
  const unlinkIssue = useUnlinkWorkspaceIssue(workspaceId);
  const discussion = useWorkspaceActivity(workspaceId, featureEnabled);
  const commentPage = discussion.data?.pages[0];

  // `?view=issues` lands on Workspace Details: scroll this section into view
  // once per arrival (each navigation has its own location key).
  const sectionRef = useRef<HTMLElement | null>(null);
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const wantIssues = searchParams.get("view") === "issues";
  const scrolledForNavigation = useRef<string | null>(null);
  useEffect(() => {
    if (!wantIssues || !sectionRef.current) return;
    if (scrolledForNavigation.current === location.key) return;
    scrolledForNavigation.current = location.key;
    sectionRef.current.scrollIntoView?.({ block: "start" });
    // The section mounts once instance info has loaded.
  }, [wantIssues, location.key, feature.status]);

  const issueSearch = useIssueSearchText();
  const [roleOverride, setRoleOverride] =
    useState<WorkspaceIssueLinkRole | null>(null);
  const [unlinkingId, setUnlinkingId] = useState<string | null>(null);

  const linkList = links.status === "ok" ? links.data.links : [];
  const hasPrimary = linkList.some((link) => link.role === "primary");
  const role: WorkspaceIssueLinkRole =
    roleOverride ?? (hasPrimary ? "related" : "primary");

  const conflict = linkConflictOf(linkIssue.error);
  const linkError = linkIssue.error && !conflict ? linkIssue.error : null;

  // The link form searches like every other issue surface
  // (g5.issue.search.one-path); state All, so closed issues can be linked.
  const searchEnabled =
    featureEnabled && canWrite && !linkFormBlocked && issueSearch.query !== "";
  const search = useExternalIssueSearch(
    membership?.orgId ?? "",
    issueSearch.query,
    "all",
    "any",
    searchEnabled,
  );
  const linkedIssues = new Set(linkList.map(issueIdentity));
  // The result the last link request was for: its conflict or error shows.
  const [attempted, setAttempted] = useState<ExternalIssueSummaryDTO | null>(
    null,
  );

  const submit = async (
    issue: ExternalIssueSummaryDTO,
    submitRole: WorkspaceIssueLinkRole,
  ) => {
    setAttempted(issue);
    try {
      await linkIssue.mutateAsync({ ref: issue.displayKey, role: submitRole });
      setRoleOverride(null);
      setAttempted(null);
      linkIssue.reset();
    } catch {
      // The conflict or error notice below explains the failure.
    }
  };

  const unlink = async (link: WorkspaceIssueLinkDTO): Promise<boolean> => {
    setUnlinkingId(link.id);
    try {
      await unlinkIssue.mutateAsync(link.id);
      unlinkIssue.reset();
      return true;
    } catch {
      return false;
    }
  };

  // Without the feature the section is absent; loading renders nothing
  // until availability is known. Switched off, it appears only while there
  // are links left to unlink.
  if (feature.status === "loading") return null;
  if (feature.status === "disabled") {
    if (!cleanupOnly || links.status !== "ok" || linkList.length === 0) {
      return null;
    }
    return (
      <section
        ref={sectionRef}
        id={WORKSPACE_ISSUES_SECTION_ID}
        data-testid="workspace-issues-cleanup"
        aria-labelledby={headingId}
        className={SECTION_CLASSES}
      >
        <h3 id={headingId} className={HEADING_CLASSES}>
          Issues
        </h3>
        <p className="mb-4 mt-1 max-w-2xl text-[13px] leading-relaxed text-[var(--g5-text1)]">
          Issues are off for this organization. These links remain and can be
          unlinked; their issues are not shown.
        </p>
        <ul
          data-testid="workspace-issues-list"
          aria-label="Linked issues"
          className="divide-y divide-[var(--g5-stroke)] overflow-hidden rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]"
        >
          {linkList.map((link) => (
            <IssueLinkRow
              key={link.id}
              link={link}
              canWrite={canWrite}
              writeBlockedReason={writeBlockedReason}
              onOpenIssue={onOpenIssue}
              onUnlink={unlink}
              unlinking={unlinkIssue.isLoading && unlinkingId === link.id}
              unlinkError={unlinkingId === link.id ? unlinkIssue.error : null}
              cleanupOnly
            />
          ))}
        </ul>
      </section>
    );
  }

  if (feature.status === "error") {
    return (
      <section
        ref={sectionRef}
        id={WORKSPACE_ISSUES_SECTION_ID}
        aria-labelledby={headingId}
        className={SECTION_CLASSES}
      >
        <h3 id={headingId} className={HEADING_CLASSES}>
          Issues
        </h3>
        <IssueErrorNotice
          testId="workspace-issues-feature-error"
          presentation={describeFeatureCheckError(feature.error)}
          onRetry={feature.retry}
        />
      </section>
    );
  }

  return (
    <section
      ref={sectionRef}
      id={WORKSPACE_ISSUES_SECTION_ID}
      data-testid="workspace-issues-panel"
      aria-labelledby={headingId}
      className={SECTION_CLASSES}
    >
      <div className="mb-4 flex items-start gap-2">
        <CircleDot
          aria-hidden="true"
          className="mt-0.5 h-4 w-4 flex-none text-[var(--g5-text1)]"
        />
        <div>
          <h3 id={headingId} className={HEADING_CLASSES}>
            Issues
          </h3>
          <p className="mt-1 max-w-2xl text-[13px] leading-relaxed text-[var(--g5-text1)]">
            Issues this workspace works on. A workspace has at most one primary
            issue; unlinking never changes the issue.
          </p>
        </div>
      </div>

      {viewerOwnsWorkspace && linkFormBlockedReason && (
        <p
          data-testid="workspace-issue-link-blocked"
          role="status"
          className="mb-4 text-xs text-[var(--g5-text2)]"
        >
          {linkFormBlockedReason}
        </p>
      )}
      {canWrite && (
        <form
          role="search"
          data-testid="workspace-issue-link-form"
          aria-label="Link an issue"
          className="mb-4 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-3"
          onSubmit={(event) => {
            event.preventDefault();
            // New text is searched at once; text already searched (say,
            // after a failure) is searched again.
            const next = issueSearch.text.trim();
            if (next !== issueSearch.query) issueSearch.setQuery(next);
            else if (searchEnabled) search.refetch();
          }}
        >
          <div className="flex flex-wrap items-end gap-3">
            <div className="min-w-[240px] flex-1">
              <label
                htmlFor={refId}
                className="text-xs font-medium text-[var(--g5-text1)]"
              >
                Find an issue
              </label>
              <input
                id={refId}
                type="search"
                data-testid="workspace-issue-ref-input"
                value={issueSearch.text}
                onChange={(event) => {
                  issueSearch.setText(event.target.value);
                  if (linkIssue.error) linkIssue.reset();
                }}
                disabled={linkFormBlocked}
                title={linkFormBlockedReason ?? undefined}
                placeholder="Search, or paste ACME-12, owner/name#123 or a URL"
                autoComplete="off"
                className="mt-1 h-9 w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] disabled:opacity-60"
              />
            </div>
            <fieldset className="flex h-9 items-center gap-3">
              <legend className="sr-only">Link role</legend>
              {(["primary", "related"] as const).map((option) => (
                <label
                  key={option}
                  className="inline-flex cursor-pointer items-center gap-1.5 text-sm text-[var(--g5-text0)]"
                >
                  <input
                    type="radio"
                    name={roleName}
                    value={option}
                    data-testid={`workspace-issue-role-${option}`}
                    checked={role === option}
                    onChange={() => setRoleOverride(option)}
                    className="accent-[var(--g5-info)]"
                  />
                  {option === "primary" ? "Primary" : "Related"}
                </label>
              ))}
            </fieldset>
          </div>
          {searchEnabled && (
            <div
              data-testid="workspace-issue-search-results"
              className="mt-3 space-y-2"
            >
              <IssueSearchOutcome
                search={search}
                testIdPrefix="workspace-issue"
              >
                {(found) => (
                  <ul
                    aria-label="Matching issues"
                    className="max-h-72 divide-y divide-[var(--g5-stroke)] overflow-y-auto rounded border border-[var(--g5-stroke)]"
                  >
                    {found.map((issue) => (
                      <IssueSearchResultRow
                        key={issueIdentity(issue)}
                        issue={issue}
                        linked={linkedIssues.has(issueIdentity(issue))}
                        linking={
                          linkIssue.isLoading &&
                          attempted !== null &&
                          issueIdentity(attempted) === issueIdentity(issue)
                        }
                        disabled={linkIssue.isLoading}
                        onLink={() => void submit(issue, role)}
                      />
                    ))}
                  </ul>
                )}
              </IssueSearchOutcome>
            </div>
          )}
          {conflict && attempted && (
            <IssueLinkConflictNotice
              conflict={conflict}
              orgSlug={orgSlug}
              pending={linkIssue.isLoading}
              onLinkAsRelated={() => {
                setRoleOverride("related");
                void submit(attempted, "related");
              }}
            />
          )}
          {linkError && (
            <div className="mt-3">
              <IssueErrorNotice
                testId="workspace-issue-link-error"
                presentation={describeIssueRequestError(linkError)}
              />
            </div>
          )}
        </form>
      )}

      {links.status === "loading" && (
        <p
          role="status"
          data-testid="workspace-issues-loading"
          className="text-sm text-[var(--g5-text2)]"
        >
          Loading linked issues…
        </p>
      )}
      {links.status === "error" && (
        <IssueErrorNotice
          testId="workspace-issues-error"
          presentation={describeIssueRequestError(links.error)}
          onRetry={links.refetch}
        />
      )}
      {links.status === "ok" && linkList.length === 0 && (
        <p
          data-testid="workspace-issues-empty"
          className="rounded-lg border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-5 py-5 text-center text-sm text-[var(--g5-text2)]"
        >
          No issues are linked to this workspace.
        </p>
      )}
      {links.status === "ok" && linkList.length > 0 && (
        <ul
          data-testid="workspace-issues-list"
          aria-label="Linked issues"
          className="divide-y divide-[var(--g5-stroke)] overflow-hidden rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]"
        >
          {linkList.map((link) => (
            <IssueLinkRow
              key={link.id}
              link={link}
              canWrite={canWrite}
              writeBlockedReason={writeBlockedReason}
              onOpenIssue={onOpenIssue}
              onUnlink={unlink}
              unlinking={unlinkIssue.isLoading && unlinkingId === link.id}
              unlinkError={unlinkingId === link.id ? unlinkIssue.error : null}
              cleanupOnly={false}
            />
          ))}
        </ul>
      )}
      {discussion.isError ? (
        <IssueErrorNotice
          testId="workspace-issues-comments-error"
          presentation={describeIssueRequestError(discussion.error)}
          onRetry={() => void discussion.refetch()}
        />
      ) : (
        commentPage?.composer && (
          <section
            aria-label="GitHub issue comments"
            data-testid="workspace-issues-comments"
            className="mt-6"
          >
            <h4 className={HEADING_CLASSES}>GitHub issue comments</h4>
            <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">
              Comments here are posted to the linked GitHub issue. For general
              workspace feedback, use Reviews on the Approvals page.
            </p>
            <ActivityCommentComposer
              workspaceId={workspaceId}
              composer={commentPage.composer}
              trusted={commentPage.primaryIssue?.contentTrusted ?? false}
            />
          </section>
        )
      )}
    </section>
  );
}
