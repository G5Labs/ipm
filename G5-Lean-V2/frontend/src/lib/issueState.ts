// =============================================================================
// Close or reopen a linked issue by hand (g5.external-issues.transition)
// =============================================================================
// The rail's Done box and the issue page's Mark as done / Reopen share these
// reasons and rules: a GitHub write is the person's own, under their GitHub
// account, so it needs a connection, a readable issue and no review copy.
// A native G5 issue changes in G5 and needs no connection
// (g5.issue.native.writes). The domain's switch and the repository's
// audience are checked by the route when the change is sent.
// =============================================================================

import { ApiError } from "./api";
import { isNativeIssue } from "./issuePage";
import type { GithubConnectionStatusDTO, WorkspaceIssueLinkDTO } from "./types";

export const ISSUE_STATE_CONNECT_REASON =
  "Connect GitHub to mark issues as done or reopen them";
export const ISSUE_STATE_CONNECTION_PENDING_REASON =
  "Checking your GitHub connection…";
export const ISSUE_STATE_CONNECTION_UNKNOWN_REASON =
  "Could not check your GitHub connection";
export const ISSUE_STATE_UNREADABLE_REASON =
  "The issue cannot be read, so its state cannot be changed";

/** The connection query as `useGithubConnection().status` reports it. */
export interface GithubConnectionQueryState {
  data: GithubConnectionStatusDTO | undefined;
  isPending: boolean;
  isError: boolean;
  error: Error | ApiError | null;
}

/** A link names a native issue: by its provider, else its locator. */
function linksNativeIssue(
  link: Pick<WorkspaceIssueLinkDTO, "issue"> &
    Partial<Pick<WorkspaceIssueLinkDTO, "provider" | "locator">>,
): boolean {
  return isNativeIssue({
    provider: link.provider ?? link.issue?.provider,
    locator: link.locator ?? link.issue?.locator,
  });
}

/**
 * Why a linked issue cannot be closed or reopened from here (`null` when it
 * can): a review copy or unknown reviewer, an unreadable issue, or (GitHub
 * only) no GitHub connection. The domain's switch and the repository's
 * audience are checked by the route when the change is sent.
 */
export function issueStateBlockedReason(
  link: Pick<WorkspaceIssueLinkDTO, "issue"> &
    Partial<Pick<WorkspaceIssueLinkDTO, "provider" | "locator">>,
  writeBlockedReason: string | null | undefined,
  connection: GithubConnectionQueryState,
): string | null {
  if (writeBlockedReason) return writeBlockedReason;
  if (!link.issue) return ISSUE_STATE_UNREADABLE_REASON;
  if (linksNativeIssue(link)) return null;
  if (connection.isPending) return ISSUE_STATE_CONNECTION_PENDING_REASON;
  if (connection.isError || !connection.data)
    return ISSUE_STATE_CONNECTION_UNKNOWN_REASON;
  if (!connection.data.connected) return ISSUE_STATE_CONNECT_REASON;
  return null;
}

/** What a press of the Done control does, for its title. */
export function issueStateActionTitle(
  link: Pick<WorkspaceIssueLinkDTO, "issue" | "displayKey"> &
    Partial<Pick<WorkspaceIssueLinkDTO, "provider" | "locator">>,
): string {
  if (linksNativeIssue(link)) {
    return nextIssueState(link) === "open"
      ? `Reopen ${link.displayKey}`
      : `Mark ${link.displayKey} as done`;
  }
  return nextIssueState(link) === "open"
    ? `Reopen ${link.displayKey} on GitHub as your account`
    : `Mark ${link.displayKey} as done on GitHub as your account`;
}

/** The state a press asks for: the opposite of the one shown. */
export function nextIssueState(
  link: Pick<WorkspaceIssueLinkDTO, "issue">,
): "closed" | "open" {
  return link.issue?.state === "closed" ? "open" : "closed";
}

/**
 * The write's answer was lost (no response, or none the server vouched
 * for): the change may have applied, so the issue is re-read.
 */
export function issueStateAnswerLost(error: unknown): boolean {
  return (
    !(error instanceof ApiError) || error.status === 0 || error.status >= 500
  );
}

export const ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE =
  "The change was not confirmed. The issue was re-read; check its state and try again if needed.";
