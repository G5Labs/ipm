// =============================================================================
// Workspace landing — Primary Issue (formerly Workspace task)
// =============================================================================
// The task box (ui g5.workbench.landing.task): one title row (the primary
// issue's status, the task title, the numbered link to where the issue
// lives), the opening of the task body, the collapsed Linked issues
// disclosure and Read more at the bottom. The task source is unchanged: the
// primary issue, else the workspace description, else the docs README, each
// read through its existing owner. Without a primary issue nothing is
// invented: the native title and body stand alone. Linked issues are the
// workspace's related links; each opens its own reader.
// =============================================================================

import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { MarkdownRenderer } from "../shared/MarkdownRenderer";
import {
  useExternalIssueDetail,
  useExternalIssuesFeature,
  useRouteOrgMembership,
  useWorkspaceActivity,
  useWorkspaceIssueLinks,
} from "../../hooks/useExternalIssues";
import { useFileContent } from "../../hooks/useFiles";
import { stripHtmlComments } from "../../lib/githubMarkdown";
import { isNativeIssue, nativeIssueHref } from "../../lib/issuePage";
import { issueLifecycle, type LifecycleState } from "../../lib/landingStatus";
import { useNativeAttachmentImages } from "../external-issues/NativeAttachments";
import type {
  ExternalIssueLocatorDTO,
  IssueProvider,
  WorkspaceIssueLinkDTO,
} from "../../lib/types";
import {
  ClippedPreview,
  ExternalRecordLink,
  LandingReader,
  LandingRow,
  LandingSection,
  LifecycleStatus,
  ReadMoreButton,
} from "./LandingParts";

/** The identity of an issue the landing shows: primary or linked. */
interface IssueRef {
  provider: IssueProvider;
  displayKey: string;
  canonicalUrl: string;
  locator: ExternalIssueLocatorDTO;
}

function issueRef(link: {
  provider?: IssueProvider;
  displayKey: string;
  canonicalUrl: string;
  locator: ExternalIssueLocatorDTO;
}): IssueRef {
  return {
    provider: link.provider ?? "github",
    displayKey: link.displayKey,
    canonicalUrl: link.canonicalUrl,
    locator: link.locator,
  };
}

/** `owner/name` of a GitHub display key `owner/name#12`. */
function repositoryOf(ref: IssueRef): string | null {
  if (ref.provider !== "github") return null;
  const hash = ref.displayKey.lastIndexOf("#");
  return hash > 0 ? ref.displayKey.slice(0, hash) : null;
}

/**
 * The numbered link label. GitHub issues read "GitHub issue #N"; when the
 * shown issues span several repositories the number alone is ambiguous, so
 * the repository's short name (or full name, when short names collide) is
 * added. A native issue is named by its own key.
 */
function issueLinkLabel(ref: IssueRef, repositories: string[]): string {
  if (ref.provider !== "github") return `Issue ${ref.displayKey}`;
  const repository = repositoryOf(ref);
  if (!repository || repositories.length <= 1) {
    return `GitHub issue #${ref.locator.number}`;
  }
  const short = repository.split("/").pop() ?? repository;
  const collides =
    repositories.filter((r) => (r.split("/").pop() ?? r) === short).length > 1;
  return `GitHub issue ${collides ? repository : short}#${ref.locator.number}`;
}

function IssueLink({
  issue,
  repositories,
  testId,
}: {
  issue: IssueRef;
  repositories: string[];
  testId: string;
}) {
  const native = issue.provider !== "github";
  return (
    <ExternalRecordLink
      href={native ? nativeIssueHref(issue.canonicalUrl) : issue.canonicalUrl}
      internal={native}
      label={issueLinkLabel(issue, repositories)}
      testId={testId}
    />
  );
}

function LinkedIssueRows({
  links,
  repositories,
  onOpen,
  testIdPrefix,
}: {
  links: WorkspaceIssueLinkDTO[];
  repositories: string[];
  onOpen: (link: WorkspaceIssueLinkDTO) => void;
  testIdPrefix: string;
}) {
  return (
    <ul data-testid={`${testIdPrefix}-list`}>
      {links.map((link) => (
        <LandingRow
          key={link.id}
          testId={`${testIdPrefix}-row`}
          lead={
            <button
              type="button"
              data-testid={`${testIdPrefix}-open`}
              onClick={() => onOpen(link)}
              className="max-w-full truncate text-left text-sm text-[var(--g5-text0)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
            >
              {link.issue?.title ?? link.displayKey}
            </button>
          }
          trailing={
            <>
              <LifecycleStatus state={issueLifecycle(link.issue)} />
              <IssueLink
                issue={issueRef(link)}
                repositories={repositories}
                testId={`${testIdPrefix}-github`}
              />
            </>
          }
        />
      ))}
    </ul>
  );
}

function TitleRow({
  status,
  title,
  link,
  headingLevel = "h3",
}: {
  status: LifecycleState | null;
  title: string;
  link?: React.ReactNode;
  headingLevel?: "h2" | "h3";
}) {
  const Heading = headingLevel;
  return (
    <div className="flex flex-wrap items-start gap-x-3 gap-y-2">
      <div className="flex min-w-0 flex-1 basis-64 items-baseline gap-2.5">
        {status && (
          <LifecycleStatus state={status} testId="landing-task-status" />
        )}
        <Heading
          data-testid="landing-task-title"
          className="min-w-0 break-words text-lg font-semibold text-[var(--g5-text0)]"
        >
          {title}
        </Heading>
      </div>
      {link}
    </div>
  );
}

export function WorkspaceTaskSection({
  workspaceId,
  workspaceName,
  description,
  docsRepoId,
  disclosure,
}: {
  workspaceId: string;
  workspaceName: string;
  /** Workspace description: the task body when no issue gives one. */
  description: string | null;
  /** Docs repository for the README fallback; `null` skips it. */
  docsRepoId: string | null;
  /** The landing's Primary Issue disclosure (g5.workbench.landing.task). */
  disclosure?: { open: boolean; onToggle: () => void };
}) {
  const [reader, setReader] = useState<
    { kind: "task" } | { kind: "issue"; link: WorkspaceIssueLinkDTO } | null
  >(null);
  const [linkedOpen, setLinkedOpen] = useState(false);
  const membership = useRouteOrgMembership();
  const orgId = membership?.orgId ?? "";
  // Issue links are read only where the organization offers issues; the
  // Activity feed's first page also names the primary issue (shared cache
  // key) and stands in when the link list is off or cannot be read.
  const feature = useExternalIssuesFeature(orgId);
  const linksOffered = feature.status === "enabled";
  const links = useWorkspaceIssueLinks(workspaceId, linksOffered);
  const activity = useWorkspaceActivity(workspaceId);
  const linksRead = linksOffered && links.status === "ok";
  const linkList = linksRead ? links.data.links : [];
  const primaryLink = linkList.find((l) => l.role === "primary") ?? null;
  const activityPrimary = activity.data?.pages[0]?.primaryIssue ?? null;
  const primary: IssueRef | null = primaryLink
    ? issueRef(primaryLink)
    : !linksRead && activityPrimary
      ? issueRef(activityPrimary)
      : null;
  const related = linkList.filter(
    (l) =>
      l.role === "related" &&
      !(
        primary &&
        l.locator.repositoryId === primary.locator.repositoryId &&
        l.locator.number === primary.locator.number
      ),
  );
  const repositories = [
    ...new Set(
      [primary, ...related.map(issueRef)]
        .map((ref) => (ref ? repositoryOf(ref) : null))
        .filter((r): r is string => !!r),
    ),
  ];
  const detail = useExternalIssueDetail(
    orgId,
    primary?.locator.repositoryId ?? "",
    primary?.locator.number ?? 0,
    primary !== null,
  );

  // Whether a primary issue exists is unknown until either read resolves.
  const primaryResolved =
    linksRead ||
    (feature.status !== "loading" &&
      (!linksOffered || links.status === "error") &&
      activity.status !== "pending");
  const issueBody =
    primary && detail.status === "ok" ? (detail.data.body ?? "").trim() : "";
  const issuePending =
    !primaryResolved || (primary !== null && detail.status === "loading");
  const descriptionText = description?.trim() ?? "";
  // README is read only once the issue and description gave nothing.
  const readmeNeeded =
    !issuePending && issueBody === "" && descriptionText === "" && !!docsRepoId;
  const readme = useFileContent(
    workspaceId,
    readmeNeeded ? (docsRepoId ?? "") : "",
    "README.md",
  );
  const readmeText =
    readmeNeeded && readme.status === "ok" ? readme.data.trim() : "";

  let body: {
    source: "issue" | "description" | "readme" | "none";
    text: string;
  };
  if (issueBody) body = { source: "issue", text: issueBody };
  else if (issuePending) body = { source: "none", text: "" };
  else if (descriptionText)
    body = { source: "description", text: descriptionText };
  else if (readmeText) body = { source: "readme", text: readmeText };
  else body = { source: "none", text: "" };
  const bodyPending =
    issuePending || (readmeNeeded && readme.status === "loading");
  const trusted =
    body.source !== "issue" ||
    (detail.status === "ok" && detail.data.contentTrusted);

  const primaryIssue =
    primaryLink?.issue ?? (detail.status === "ok" ? detail.data : null);
  const title =
    (detail.status === "ok" ? detail.data.title : null) ??
    primaryLink?.issue?.title ??
    (primary
      ? primary.displayKey
      : primaryResolved
        ? workspaceName
        : "Loading task…");
  const status: LifecycleState | null = primary
    ? issueLifecycle(primaryIssue)
    : null;
  const primaryLinkButton = primary ? (
    <IssueLink
      issue={primary}
      repositories={repositories}
      testId="landing-task-github"
    />
  ) : undefined;
  // A G5 issue's attachments load through the reader's own authenticated
  // read, as on the issue page (g5.issue.native.attachments).
  const attachmentImages = useNativeAttachmentImages(primary?.canonicalUrl);
  const nativeBody =
    body.source === "issue" &&
    detail.status === "ok" &&
    isNativeIssue(detail.data);
  const rendered = (
    <MarkdownRenderer
      content={stripHtmlComments(body.text)}
      trustedContent={trusted}
      renderImage={nativeBody ? attachmentImages : undefined}
    />
  );
  const openIssue = (link: WorkspaceIssueLinkDTO) =>
    setReader({ kind: "issue", link });

  return (
    <LandingSection
      label="Primary Issue"
      testId="landing-task"
      disclosure={disclosure}
    >
      <TitleRow status={status} title={title} link={primaryLinkButton} />
      <div className="mt-2">
        {bodyPending ? (
          <p role="status" className="text-sm italic text-[var(--g5-text2)]">
            Loading task…
          </p>
        ) : body.text ? (
          <ClippedPreview testId="landing-task-preview">
            {rendered}
          </ClippedPreview>
        ) : (
          <p
            data-testid="landing-task-empty"
            className="text-sm italic text-[var(--g5-text2)]"
          >
            No task description recorded.
          </p>
        )}
      </div>
      {primary && detail.status === "error" && (
        <p role="status" className="mt-2 text-xs text-[var(--g5-warn)]">
          The issue could not be read from its tracker.{" "}
          <button
            type="button"
            data-testid="landing-task-issue-retry"
            className="text-[var(--g5-info)] hover:underline"
            onClick={() => detail.refetch()}
          >
            Retry
          </button>
        </p>
      )}
      {linksOffered && links.status === "error" && (
        <p role="status" className="mt-2 text-xs text-[var(--g5-warn)]">
          Linked issues could not be read.
        </p>
      )}
      {related.length > 0 && (
        <div className="mt-3 border-t border-[var(--g5-stroke)] pt-2">
          <button
            type="button"
            data-testid="landing-linked-issues-toggle"
            aria-expanded={linkedOpen}
            onClick={() => setLinkedOpen((open) => !open)}
            className="inline-flex items-center gap-1 text-xs text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          >
            {linkedOpen ? (
              <ChevronDown size={14} aria-hidden="true" />
            ) : (
              <ChevronRight size={14} aria-hidden="true" />
            )}
            Linked issues ({related.length})
          </button>
          {linkedOpen && (
            <div className="mt-2">
              <LinkedIssueRows
                links={related}
                repositories={repositories}
                onOpen={openIssue}
                testIdPrefix="landing-linked-issue"
              />
            </div>
          )}
        </div>
      )}
      {(body.text || related.length > 0) && (
        <ReadMoreButton
          testId="workspace-read-task-description"
          onClick={() => setReader({ kind: "task" })}
        />
      )}
      {reader?.kind === "task" && (
        <LandingReader
          ariaLabel="Workspace task"
          contentLabel="Full task description"
          onClose={() => setReader(null)}
          testId="landing-task-reader"
          heading={
            <TitleRow
              status={status}
              title={title}
              link={primaryLinkButton}
              headingLevel="h2"
            />
          }
        >
          {body.text ? (
            rendered
          ) : (
            <p className="text-sm italic text-[var(--g5-text2)]">
              No task description recorded.
            </p>
          )}
          {related.length > 0 && (
            <div className="mt-5">
              <h3 className="mb-1 text-xs font-medium text-[var(--g5-text2)]">
                Linked issues ({related.length})
              </h3>
              <LinkedIssueRows
                links={related}
                repositories={repositories}
                onOpen={openIssue}
                testIdPrefix="landing-reader-linked-issue"
              />
            </div>
          )}
        </LandingReader>
      )}
      {reader?.kind === "issue" && (
        <LinkedIssueReader
          orgId={orgId}
          link={reader.link}
          repositories={repositories}
          onClose={() => setReader(null)}
        />
      )}
    </LandingSection>
  );
}

/** One linked issue's own identity and description; no inherited list. */
function LinkedIssueReader({
  orgId,
  link,
  repositories,
  onClose,
}: {
  orgId: string;
  link: WorkspaceIssueLinkDTO;
  repositories: string[];
  onClose: () => void;
}) {
  const detail = useExternalIssueDetail(
    orgId,
    link.locator.repositoryId,
    link.locator.number,
  );
  const issue = detail.status === "ok" ? detail.data : link.issue;
  const attachmentImages = useNativeAttachmentImages(link.canonicalUrl);
  return (
    <LandingReader
      ariaLabel="Linked issue"
      contentLabel="Full issue description"
      onClose={onClose}
      testId="landing-linked-issue-reader"
      heading={
        <TitleRow
          status={issueLifecycle(issue)}
          title={issue?.title ?? link.displayKey}
          headingLevel="h2"
          link={
            <IssueLink
              issue={issueRef(link)}
              repositories={repositories}
              testId="landing-linked-issue-reader-github"
            />
          }
        />
      }
    >
      {detail.status === "loading" ? (
        <p role="status" className="text-sm italic text-[var(--g5-text2)]">
          Loading issue…
        </p>
      ) : detail.status === "error" ? (
        <p role="status" className="text-sm text-[var(--g5-warn)]">
          The issue could not be read from its tracker.
        </p>
      ) : detail.data.body?.trim() ? (
        <MarkdownRenderer
          content={stripHtmlComments(detail.data.body)}
          trustedContent={detail.data.contentTrusted}
          renderImage={
            isNativeIssue(detail.data) ? attachmentImages : undefined
          }
        />
      ) : (
        <p className="text-sm italic text-[var(--g5-text2)]">
          No description provided.
        </p>
      )}
    </LandingReader>
  );
}
