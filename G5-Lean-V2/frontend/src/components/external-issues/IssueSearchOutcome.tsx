// =============================================================================
// Issue search outcome — what an organization issue search returned
// =============================================================================
// The loading, failure, per-domain failure, empty and truncated states of a
// search over the organization's issue domains, shared by the surfaces that
// find an issue to link (g5.issue.search.one-path). A failed domain is shown
// beside the others' matches, never as zero matches
// (g5.issue.search-completeness).
// =============================================================================

import type { ReactNode } from "react";

import type { ExternalIssueSearchState } from "../../hooks/useExternalIssues";
import {
  describeIssueError,
  describeIssueRequestError,
} from "../../lib/externalIssueErrors";
import type { ExternalIssueSummaryDTO } from "../../lib/types";
import { IssueErrorNotice } from "./ExternalIssueParts";

export function IssueSearchOutcome({
  search,
  testIdPrefix,
  children,
}: {
  search: ExternalIssueSearchState;
  /** `<prefix>-search-error`, `<prefix>-domain-error-<domainId>`, … */
  testIdPrefix: string;
  /** The matches, most recently updated first. */
  children: (results: ExternalIssueSummaryDTO[]) => ReactNode;
}) {
  const { state } = search;
  if (state.status === "loading") {
    return (
      <p role="status" className="text-sm text-[var(--g5-text2)]">
        Searching issues…
      </p>
    );
  }
  if (state.status === "error") {
    return (
      <IssueErrorNotice
        testId={`${testIdPrefix}-search-error`}
        presentation={describeIssueRequestError(state.error)}
        onRetry={search.refetch}
      />
    );
  }
  const { results, domainErrors, domainCount, truncated } = state.data;
  const everyDomainFailed =
    domainErrors.length > 0 &&
    domainErrors.length >= domainCount &&
    results.length === 0;
  return (
    <>
      {domainErrors.map((error) => {
        const presentation = describeIssueError(
          error.issueError,
          error.message,
        );
        return (
          <IssueErrorNotice
            testId={`${testIdPrefix}-domain-error-${error.domainId}`}
            key={error.domainId}
            compact
            presentation={{
              ...presentation,
              title: `${error.displayKey}: ${presentation.title}`,
            }}
            onRetry={search.refetch}
          />
        );
      })}
      {everyDomainFailed ? (
        <p role="status" className="text-sm text-[var(--g5-text2)]">
          Issue search is unavailable.
        </p>
      ) : domainCount === 0 ? (
        <p className="text-sm text-[var(--g5-text2)]">
          No issue tracker is configured for this organization.
        </p>
      ) : results.length === 0 ? (
        <p
          data-testid={`${testIdPrefix}-no-matches`}
          className="text-sm text-[var(--g5-text2)]"
        >
          {domainErrors.length
            ? "No matching issues in the repositories that could be searched."
            : "No matching issues."}
        </p>
      ) : (
        children(results)
      )}
      {truncated && (
        <p className="mt-2 text-xs text-[var(--g5-text2)]">
          Showing the most recently updated matches. Refine the search to see
          others.
        </p>
      )}
    </>
  );
}
