// =============================================================================
// An issue's own page in G5
// =============================================================================
// `/:org/issues/github/:repositoryId/:number` shows one GitHub issue of the
// organization's issue domains outside any workspace; Home's Create opens it
// for the new issue (g5.workbench.issues.create). A native G5 issue's page is
// `/:org/issues/g5/:number`. On either page members with write permission
// change the issue (g5.issue.native.page): a GitHub issue as their
// connected account, while its write target allows it.
// =============================================================================

import type {
  AsyncState,
  ExternalIssueCommentDTO,
  ExternalIssueLocatorDTO,
  IssueProvider,
  IssueWriteTargetDTO,
  WorkspaceIssueLinkDTO,
} from "./types";
export const ISSUE_PAGE_PATH = "/:org/issues/github/:repositoryId/:number";
export const NATIVE_ISSUE_PAGE_PATH = "/:org/issues/g5/:number";

/** The locator `repositoryId` of every native G5 issue. */
export const NATIVE_ISSUE_REPOSITORY_ID = "g5";

export function issuePagePath(
  orgSlug: string,
  repositoryId: string,
  number: number,
): string {
  if (repositoryId === NATIVE_ISSUE_REPOSITORY_ID) {
    return `/${orgSlug}/issues/g5/${number}`;
  }
  return `/${orgSlug}/issues/github/${encodeURIComponent(repositoryId)}/${number}`;
}

/**
 * Whether something names a native G5 issue: by its `provider` when the
 * server sent one, else by its locator (`repositoryId` "g5").
 */
export function isNativeIssue(issue: {
  provider?: IssueProvider | null;
  locator?: ExternalIssueLocatorDTO | null;
  repositoryId?: string | null;
}): boolean {
  if (issue.provider) return issue.provider === "g5";
  const repositoryId = issue.locator?.repositoryId ?? issue.repositoryId;
  return repositoryId === NATIVE_ISSUE_REPOSITORY_ID;
}

/** The tracker's name for copy: "G5" or "GitHub". */
export function issueTrackerName(native: boolean): string {
  return native ? "G5" : "GitHub";
}

/**
 * The in-app path of a native issue's absolute page URL (the server builds
 * it from the deployment's public address), for client-side navigation.
 */
export function nativeIssueHref(canonicalUrl: string): string {
  try {
    const url = new URL(canonicalUrl, window.location.origin);
    return `${url.pathname}${url.search}${url.hash}`;
  } catch {
    return canonicalUrl;
  }
}

/** An issue number route segment: decimal digits only, else 0. Shared by the
 *  issue page and the workspace page's `/issue/:repositoryId/:number` route
 *  (g5.workbench.tabs). */
export function parseIssueNumberParam(
  value: string | null | undefined,
): number {
  return value && /^\d+$/.test(value) ? Number(value) : 0;
}

/** The workspace's link to an issue, by repository and number. */
export function findWorkspaceIssueLink(
  links: readonly WorkspaceIssueLinkDTO[],
  locator: ExternalIssueLocatorDTO,
): WorkspaceIssueLinkDTO | undefined {
  return links.find(
    (candidate) =>
      candidate.locator.repositoryId === locator.repositoryId &&
      candidate.locator.number === locator.number,
  );
}

/**
 * The organization's Issues settings: the Issues switch and issue domains,
 * on the Admin page (g5.issue.native.domain).
 */
export function issueSettingsPath(orgSlug: string): string {
  return `/${orgSlug}/base?view=admin`;
}

/** What the viewer may do to an issue on its page. */
export interface IssuePageWriter {
  orgId: string;
  userId: string;
  onReload: () => void;
  /** A GitHub issue's write target; `null` on a native issue. */
  target: (AsyncState<IssueWriteTargetDTO> & { refetch: () => void }) | null;
}

/**
 * The GitHub write target when writes are possible now; `null` while it
 * loads, failed or blocks them. A native issue needs none.
 */
export function readyWriteTarget(
  writer: IssuePageWriter,
): IssueWriteTargetDTO | null {
  const target = writer.target;
  return target?.status === "ok" && target.data.state === "ready"
    ? target.data
    : null;
}

/** Writes are possible now: always on a native issue. */
export function canWriteNow(writer: IssuePageWriter): boolean {
  return writer.target === null || readyWriteTarget(writer) !== null;
}

/**
 * The viewer's own comment: on a native issue by G5 user; on GitHub by the
 * connected account's login (the server checks by account id).
 */
export function isOwnComment(
  writer: IssuePageWriter,
  comment: ExternalIssueCommentDTO,
): boolean {
  if (writer.target === null) {
    return !!comment.author?.userId && comment.author.userId === writer.userId;
  }
  const login = readyWriteTarget(writer)?.githubLogin;
  return (
    !!login &&
    !!comment.author?.login &&
    comment.author.login.toLowerCase() === login.toLowerCase()
  );
}
