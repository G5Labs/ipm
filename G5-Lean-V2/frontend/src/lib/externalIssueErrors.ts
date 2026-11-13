// =============================================================================
// Issue error presentation (GitHub domains and the native G5 tracker)
// =============================================================================
// The API reports typed issue failures as `error.details.issueError`, per-domain
// search failures in `domainErrors`, and per-link failures in `issueError`.
// Every failure renders as an explicit error — never as an empty result.
// =============================================================================

import { ApiError } from "./api";
import type {
  ExternalIssueErrorCode,
  ExternalIssuePrimaryWorkspaceDTO,
  IssueDomainUnavailableReason,
} from "./types";

export interface IssueErrorPresentation {
  /** Code-specific explanation, safe to show as the primary message. */
  title: string;
  /** Server-provided detail, when it adds information beyond the title. */
  detail: string | null;
}

const ISSUE_ERROR_TITLES: Record<ExternalIssueErrorCode, string> = {
  unavailable: "The issue tracker did not respond in time. Try again shortly.",
  rate_limited:
    "GitHub is limiting how often G5 can search or read issues. Wait a minute and try again.",
  access_denied: "GitHub denied the G5 GitHub App access to this repository.",
  not_found: "The issue or repository could not be found.",
  outside_domain:
    "This repository is not configured as an issue domain for the organization.",
  not_an_issue: "This reference is a pull request, not an issue.",
  binding_needs_attention:
    "The organization's GitHub App binding needs attention. An organization admin can review it on the GitHub App settings page.",
  invalid_reference:
    "Enter an issue URL or a reference like owner/name#123 or ACME-12.",
  invalid_query:
    "GitHub could not run this search. Check the search terms and qualifiers.",
};

const GENERIC_TITLE = "The issue request could not be completed.";

function isKnownIssueErrorCode(code: string): code is ExternalIssueErrorCode {
  return Object.prototype.hasOwnProperty.call(ISSUE_ERROR_TITLES, code);
}

/** Describe an issue failure code plus its server message. */
export function describeIssueError(
  code: string | null | undefined,
  message?: string | null,
): IssueErrorPresentation {
  const title =
    code && isKnownIssueErrorCode(code)
      ? ISSUE_ERROR_TITLES[code]
      : GENERIC_TITLE;
  const trimmed = message?.trim() ?? "";
  return {
    title,
    detail: trimmed.length > 0 && trimmed !== title ? trimmed : null,
  };
}

/** The `details.issueError` code of a failed request, if any. */
export function issueErrorCodeOf(error: unknown): string | null {
  if (!(error instanceof ApiError)) return null;
  const code = error.details.issueError;
  return typeof code === "string" && code.length > 0 ? code : null;
}

/** Describe any failed issue request (typed or not). */
export function describeIssueRequestError(
  error: unknown,
): IssueErrorPresentation {
  const message = error instanceof Error ? error.message : null;
  return describeIssueError(issueErrorCodeOf(error), message);
}

/**
 * One line for a typed issue error (`details.issueError`): the title, with
 * the detail when there is one. `null` for any other failure.
 */
export function describeTypedIssueError(error: unknown): string | null {
  if (issueErrorCodeOf(error) === null) return null;
  const presentation = describeIssueRequestError(error);
  return presentation.detail
    ? `${presentation.title} ${presentation.detail}`
    : presentation.title;
}

/** The caller must connect their GitHub account first (typed 409). */
export function isGithubConnectionRequired(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.status === 409 &&
    error.details.code === "github_connection_required"
  );
}

/** Instance info could not be loaded, so availability is unknown. */
export function describeFeatureCheckError(
  error: unknown,
): IssueErrorPresentation {
  return {
    title: "Could not check whether issues are available.",
    detail: error instanceof Error ? error.message : null,
  };
}

export const ISSUE_DOMAIN_UNAVAILABLE_REASONS: Record<
  IssueDomainUnavailableReason,
  string
> = {
  no_binding: "The GitHub App is not connected.",
  binding_inactive: "The GitHub App binding is disconnected.",
  repository_uncovered:
    "The GitHub App binding no longer covers this repository.",
  issues_permission_missing:
    "The GitHub App installation lacks the Issues permission.",
};

export function describeDomainUnavailable(
  reason: IssueDomainUnavailableReason | null,
): string {
  return reason
    ? ISSUE_DOMAIN_UNAVAILABLE_REASONS[reason]
    : "This issue domain is unavailable.";
}

export type LinkConflict =
  | { code: "already_linked" }
  | { code: "workspace_has_primary" }
  | {
      code: "issue_has_primary";
      ownerWorkspace: ExternalIssuePrimaryWorkspaceDTO | null;
    };

function parseOwnerWorkspace(
  value: unknown,
): ExternalIssuePrimaryWorkspaceDTO | null {
  if (!value || typeof value !== "object") return null;
  const owner = value as Record<string, unknown>;
  if (
    owner.visible === true &&
    typeof owner.workspaceId === "string" &&
    typeof owner.name === "string"
  ) {
    return { visible: true, workspaceId: owner.workspaceId, name: owner.name };
  }
  if (owner.visible === false) return { visible: false };
  return null;
}

/** The typed 409 conflict of a failed link request, if it is one. */
export function linkConflictOf(error: unknown): LinkConflict | null {
  if (!(error instanceof ApiError) || error.status !== 409) return null;
  switch (error.details.code) {
    case "already_linked":
      return { code: "already_linked" };
    case "workspace_has_primary":
      return { code: "workspace_has_primary" };
    case "issue_has_primary":
      return {
        code: "issue_has_primary",
        ownerWorkspace: parseOwnerWorkspace(error.details.ownerWorkspace),
      };
    default:
      return null;
  }
}

/**
 * Explanation of a fork request refused because of its issue: a typed issue
 * error, or a primary claim another workspace already holds. `null` when the
 * failure is not about the issue.
 */
export function describeForkIssueError(error: unknown): string | null {
  const conflict = linkConflictOf(error);
  if (conflict?.code === "issue_has_primary") {
    const owner =
      conflict.ownerWorkspace?.visible === true
        ? conflict.ownerWorkspace.name
        : "another workspace";
    return `This issue is already the primary issue of ${owner}. Choose Related to link it to the new workspace anyway.`;
  }
  return describeTypedIssueError(error);
}
