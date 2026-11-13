// =============================================================================
// Messages for refused issue comments and issue creations
// =============================================================================
// Why a comment composer cannot post, and why a submission or edit was
// refused, from the API's typed `details.code`. Issue creation and the issue
// page's edits and state changes pass the same guards
// (g5.external-issues.creation.admission), so their reasons live here too.
// =============================================================================

import { ApiError } from "./api";
import { describeTypedIssueError } from "./externalIssueErrors";
import {
  ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE,
  issueStateAnswerLost,
} from "./issueState";
import type {
  AsyncState,
  CommentComposerState,
  IssueCreateTargetDTO,
} from "./types";

export const EXTERNAL_ISSUES_OFF_MESSAGE =
  "Issues are off for this organization.";

export const BLOCKED_REASONS: Record<
  Exclude<CommentComposerState, "ready" | "connection_required">,
  string
> = {
  comments_disabled:
    "G5 comments are not enabled for this issue's repository. An organization admin can enable them in the GitHub App settings.",
  public_destination:
    "Read-only integration: public destination. G5 does not post to public repositories; your draft is kept.",
  visibility_unknown:
    "GitHub did not report this repository's visibility, so posting is disabled; your draft is kept.",
  unavailable:
    "GitHub is unavailable right now; your draft is kept. Try again shortly.",
};

/**
 * Why the issue page cannot change a GitHub issue (edit, close or reopen),
 * by its write target's state.
 */
export const ISSUE_WRITE_BLOCKED_REASONS: Record<
  Exclude<CommentComposerState, "ready" | "connection_required">,
  string
> = {
  comments_disabled:
    "G5 comments and changes are not enabled for this issue's repository. An organization admin can enable them in the GitHub App settings.",
  public_destination:
    "Read-only integration: public destination. G5 does not change issues in public repositories.",
  visibility_unknown:
    "GitHub did not report this repository's visibility, so G5 cannot change this issue.",
  unavailable:
    "GitHub is unavailable right now, so this issue cannot be changed. Try again shortly.",
};

export const OUTCOME_UNKNOWN_COMMENT_MESSAGE =
  "GitHub did not confirm the comment. Your draft is kept; posting again is safe: G5 checks for it first and will not post it twice.";

export const COMMENT_EDIT_UNCONFIRMED_MESSAGE =
  "GitHub did not confirm the change; saving again is safe.";

/** Why a submission was refused, from the API's typed error details. */
export function describePostError(error: unknown): string {
  if (!(error instanceof ApiError)) {
    return "The comment could not be posted. Your draft is kept.";
  }
  switch (error.details.code) {
    case "destination_changed":
      return "The workspace's primary issue changed. Review the new destination and post again.";
    case "comments_disabled":
      return BLOCKED_REASONS.comments_disabled;
    case "public_destination":
      return BLOCKED_REASONS.public_destination;
    case "visibility_unknown":
      return BLOCKED_REASONS.visibility_unknown;
    case "github_connection_required":
      return "Connect your GitHub account to comment.";
    case "no_primary_issue":
      return "The workspace no longer has a primary issue.";
    case "secret_detected":
      return `The comment appears to contain a secret (${secretKinds(error)}). Remove it before posting.`;
    case "idempotency_key_reused":
      return "A different comment was already posted with this submission's reference. Post again to send this text as a new comment.";
    default:
      return error.message || "The comment could not be posted.";
  }
}

/** The secret kinds a refusal names; never the matched text. */
function secretKinds(error: ApiError): string {
  return Array.isArray(error.details.kinds) && error.details.kinds.length > 0
    ? (error.details.kinds as string[]).join(", ")
    : "a secret";
}

export const ISSUE_CHANGED_MESSAGE =
  "This issue changed since you opened it. Reload to see the latest version, then edit again.";
export const COMMENT_CHANGED_MESSAGE =
  "This comment changed since you opened the editor. Reload to edit the latest version.";

/**
 * Why an issue page write (an edit of the issue or of one's own comment, or
 * a native state change) was refused, from the API's typed error.
 */
export function describeIssueWriteError(
  error: unknown,
  fallback: string,
): string {
  if (!(error instanceof ApiError)) return fallback;
  switch (error.details.code) {
    case "issue_changed":
      return ISSUE_CHANGED_MESSAGE;
    case "comment_changed":
      return COMMENT_CHANGED_MESSAGE;
    case "not_your_comment":
      return "Only your own comments can be edited.";
    case "external_issues_disabled":
      return EXTERNAL_ISSUES_OFF_MESSAGE;
    case "comments_disabled":
      return ISSUE_WRITE_BLOCKED_REASONS.comments_disabled;
    case "public_destination":
      return ISSUE_WRITE_BLOCKED_REASONS.public_destination;
    case "visibility_unknown":
      return ISSUE_WRITE_BLOCKED_REASONS.visibility_unknown;
    case "github_connection_required":
      return "Connect your GitHub account to change this issue.";
    case "secret_detected": {
      const field =
        error.details.field === "title"
          ? "title"
          : error.details.field === "body"
            ? "text"
            : "description";
      return `The ${field} appears to contain a secret (${secretKinds(error)}). Remove it before saving.`;
    }
  }
  if (error.status === 403) {
    return "You do not have permission to change this issue.";
  }
  return describeTypedIssueError(error) ?? (error.message || fallback);
}

/** The refusal means the person must reload before trying again. */
export function isStaleIssueRefusal(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.status === 409 &&
    (error.details.code === "issue_changed" ||
      error.details.code === "comment_changed")
  );
}

/**
 * Why closing or reopening a linked issue was refused, from the typed
 * error (g5.external-issues.transition shares the comment guards).
 */
export function describeIssueStateError(
  error: unknown,
  state: "closed" | "open",
): string {
  const verb = state === "closed" ? "marked as done" : "reopened";
  // No answer, or none GitHub's side could vouch for: the change may have
  // applied, and the hook re-reads the issue.
  if (issueStateAnswerLost(error) || !(error instanceof ApiError)) {
    return ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE;
  }
  switch (error.details.code) {
    case "comments_disabled":
      return "G5 comments are not enabled for this issue's repository, so G5 cannot change its state. An organization admin can enable them in the GitHub App settings.";
    case "public_destination":
      return "Read-only integration: public destination. G5 does not change issues in public repositories.";
    case "visibility_unknown":
      return "GitHub did not report this repository's visibility, so the issue was left as it is.";
    case "github_connection_required":
      return `Connect your GitHub account to have the issue ${verb}.`;
    case "external_issues_disabled":
      return EXTERNAL_ISSUES_OFF_MESSAGE;
  }
  return (
    describeTypedIssueError(error) ??
    (error.message || `The issue could not be ${verb}.`)
  );
}

/**
 * Why no issue can be created now, from the creation target (`null` when
 * it is ready). Shown as the disabled Create controls' reason.
 */
export function describeCreateTargetBlocked(
  target: IssueCreateTargetDTO,
): string | null {
  const repository = target.destination?.displayKey ?? "this repository";
  switch (target.state) {
    case "ready":
      return null;
    case "no_target":
      return "No repository is set up for new issues";
    case "comments_disabled":
      return `An organization admin has not enabled G5 comments and issue creation for ${repository}`;
    case "public_destination":
      return `${repository} is public, and G5 does not create issues in public repositories`;
    case "visibility_unknown":
      return `GitHub did not report whether ${repository} is private, so G5 cannot create issues there`;
    case "connection_required":
      return "Connect GitHub to create issues";
    case "unavailable": {
      const tracker =
        target.destination?.provider === "g5" ? "G5 issues" : "GitHub";
      return target.error?.message
        ? `${tracker} could not be reached to check ${repository}: ${target.error.message}`
        : `${tracker} could not be reached to check ${repository}. Try again shortly`;
    }
  }
}

/** The blocked reason for a creation target that may still be loading. */
export function createTargetBlockedReason(
  target: AsyncState<IssueCreateTargetDTO>,
): string | null {
  if (target.status === "loading") return "Checking where new issues go…";
  if (target.status === "error") {
    if (
      target.error instanceof ApiError &&
      target.error.details.code === "external_issues_disabled"
    ) {
      return EXTERNAL_ISSUES_OFF_MESSAGE.replace(/\.$/, "");
    }
    return `Could not check where new issues go: ${target.error.message}`;
  }
  return describeCreateTargetBlocked(target.data);
}

/**
 * The parent issue was refused before anything was sent (a 422 naming the
 * `parent` field); shown at the field rather than as a general failure.
 */
export function isParentRefusal(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.status === 422 &&
    error.details.field === "parent"
  );
}

/** Why an issue creation or check was refused, from the typed error. */
export function describeCreateIssueError(error: unknown): string {
  if (!(error instanceof ApiError)) {
    return "The issue could not be created. Your draft is kept.";
  }
  switch (error.details.code) {
    case "destination_changed":
      return "The organization's repository for new issues changed. Review the new destination and create again.";
    case "comments_disabled":
      return "An organization admin has not enabled G5 comments and issue creation for this repository. Your draft is kept.";
    case "public_destination":
      return "G5 does not create issues in public repositories. Your draft is kept.";
    case "visibility_unknown":
      return "GitHub did not report the repository's visibility, so G5 cannot create the issue. Your draft is kept.";
    case "github_connection_required":
      return "Connect your GitHub account to create issues.";
    case "no_creation_target":
      return "No repository is set up for new issues. An organization admin can choose one in the organization's Issues settings.";
    case "external_issues_disabled":
      return EXTERNAL_ISSUES_OFF_MESSAGE;
    case "secret_detected": {
      const field = error.details.field === "title" ? "title" : "description";
      return `The ${field} appears to contain a secret (${secretKinds(error)}). Remove it before creating the issue.`;
    }
    case "parent_not_found":
      return "The parent issue could not be found.";
    case "parent_not_an_issue":
      return "The parent is a pull request, not an issue.";
    case "parent_outside_destination":
      return "The parent must be an issue where the new issue is created.";
    case "invalid_parent":
      return "Enter the parent as #123, owner/repo#123, ACME-3 or an issue URL.";
  }
  const typed = describeTypedIssueError(error);
  if (typed !== null) return typed;
  // Field validation: the server names the field and the rule.
  const detail = error.details.error;
  if (error.status === 422 && typeof detail === "string" && detail) {
    return `${detail.charAt(0).toUpperCase()}${detail.slice(1)}.`;
  }
  return error.message || "The issue could not be created.";
}
