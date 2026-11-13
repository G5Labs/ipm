// =============================================================================
// Issue presentation parts shared by Home search, the workspace Issues
// section of Workspace Details, and the issue view. A native G5 issue's
// people are G5 members (shown by name) and its link opens its G5 page.
// =============================================================================

import React from "react";
import { Link } from "react-router-dom";
import { CircleAlert, ExternalLink, RefreshCw, X } from "lucide-react";

import { Chip, classNames } from "../ui/UI";
import type { IssueErrorPresentation } from "../../lib/externalIssueErrors";
import { labelSwatchColor } from "../../lib/issueLabels";
import { nativeIssueHref } from "../../lib/issuePage";
import { issueUserLabel } from "../../lib/nativeIssues";
import type {
  ExternalIssueLabelDTO,
  ExternalIssueUserDTO,
} from "../../lib/types";

export function IssueStateChip({
  state,
  stateReason,
}: {
  state: "open" | "closed";
  stateReason?: string | null;
}) {
  const reason =
    state === "closed" && stateReason === "not_planned"
      ? "Closed · not planned"
      : null;
  return (
    <Chip
      data-testid="external-issue-state"
      state={state === "open" ? "ok" : "muted"}
      text={reason ?? (state === "open" ? "Open" : "Closed")}
    />
  );
}

/**
 * An issue's assignees. Compact rows name only the people (screen readers
 * hear the label); `labelled` shows the "Assignees" label, as the issue view
 * does.
 */
export function IssueAssignees({
  assignees,
  labelled = false,
}: {
  assignees: ExternalIssueUserDTO[];
  labelled?: boolean;
}) {
  const names = assignees
    .map((assignee) => issueUserLabel(assignee))
    .join(", ");
  if (labelled) {
    return (
      <span data-testid="external-issue-assignees" className="text-xs">
        <span className="font-medium text-[var(--g5-text2)]">Assignees:</span>{" "}
        <span
          className={
            names ? "text-[var(--g5-text1)]" : "text-[var(--g5-text2)]"
          }
        >
          {names || "None"}
        </span>
      </span>
    );
  }
  if (assignees.length === 0) {
    return (
      <span
        data-testid="external-issue-assignees"
        className="text-xs text-[var(--g5-text2)]"
      >
        Unassigned
      </span>
    );
  }
  return (
    <span
      data-testid="external-issue-assignees"
      className="text-xs text-[var(--g5-text1)]"
    >
      <span className="sr-only">Assignees: </span>
      {names}
    </span>
  );
}

/**
 * An issue's labels as coloured chips. With `onRemove` (the label picker's
 * selection) each chip has a button that drops it.
 */
export function IssueLabels({
  labels,
  onRemove,
  removeDisabled = false,
  testId = "external-issue-labels",
}: {
  labels: ExternalIssueLabelDTO[];
  onRemove?: (name: string) => void;
  removeDisabled?: boolean;
  testId?: string;
}) {
  if (labels.length === 0) return null;
  return (
    <ul
      data-testid={testId}
      aria-label="Labels"
      className="flex flex-wrap gap-1"
    >
      {labels.map((label) => {
        const color = labelSwatchColor(label.color);
        return (
          <li
            key={label.name}
            title={label.description || undefined}
            className="inline-flex items-center gap-1 rounded-full border border-[var(--g5-stroke)] px-2 py-0.5 text-[11px] text-[var(--g5-text1)]"
          >
            {color && (
              <span
                aria-hidden="true"
                className="h-2 w-2 shrink-0 rounded-full"
                style={{ backgroundColor: color }}
              />
            )}
            {label.name}
            {onRemove && (
              <button
                type="button"
                data-testid={`${testId}-remove-${label.name}`}
                aria-label={`Remove label ${label.name}`}
                disabled={removeDisabled}
                onClick={() => onRemove(label.name)}
                className="-mr-1 inline-flex h-3.5 w-3.5 items-center justify-center rounded-full text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] disabled:opacity-50"
              >
                <X aria-hidden="true" className="h-2.5 w-2.5" />
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}

const OPEN_LINK_CLASSES =
  "inline-flex flex-none items-center gap-1 rounded border border-[var(--g5-stroke)] px-2.5 py-1 text-xs font-medium text-[var(--g5-text0)] no-underline hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

export function OpenInGithubLink({
  href,
  label = "Open in GitHub",
  className,
  testId = "external-issue-open-github",
}: {
  href: string;
  label?: string;
  className?: string;
  testId?: string;
}) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      data-testid={testId}
      className={classNames(OPEN_LINK_CLASSES, className)}
    >
      <ExternalLink aria-hidden="true" className="h-3.5 w-3.5" />
      {label}
    </a>
  );
}

/**
 * Open an issue where it lives: a GitHub issue in GitHub (a new tab), a
 * native one on its G5 page (in the app).
 */
export function OpenIssueLink({
  native,
  href,
  githubLabel,
  nativeLabel = "Open issue page",
  className,
  testId = "external-issue-open-github",
}: {
  native: boolean;
  href: string;
  githubLabel?: string;
  nativeLabel?: string;
  className?: string;
  testId?: string;
}) {
  if (!native) {
    return (
      <OpenInGithubLink
        href={href}
        label={githubLabel}
        className={className}
        testId={testId}
      />
    );
  }
  return (
    <Link
      to={nativeIssueHref(href)}
      data-testid={testId}
      className={classNames(OPEN_LINK_CLASSES, className)}
    >
      {nativeLabel}
    </Link>
  );
}

export function IssueErrorNotice({
  presentation,
  testId,
  onRetry,
  compact = false,
  children,
}: {
  presentation: IssueErrorPresentation;
  testId: string;
  onRetry?: () => void;
  compact?: boolean;
  children?: React.ReactNode;
}) {
  return (
    <div
      role="alert"
      data-testid={testId}
      className={classNames(
        "flex items-start gap-2 rounded-lg border border-[rgb(255,107,107,0.3)] bg-[rgb(255,107,107,0.08)] text-[var(--g5-text1)]",
        compact ? "px-2.5 py-2 text-xs" : "px-3.5 py-3 text-sm",
      )}
    >
      <CircleAlert
        aria-hidden="true"
        className={classNames(
          "mt-0.5 shrink-0 text-[var(--g5-blocked)]",
          compact ? "h-3.5 w-3.5" : "h-4 w-4",
        )}
      />
      <div className="min-w-0 flex-1">
        <p>{presentation.title}</p>
        {presentation.detail && (
          <p className="mt-0.5 text-xs text-[var(--g5-text2)]">
            {presentation.detail}
          </p>
        )}
        {children}
      </div>
      {onRetry && (
        <button
          type="button"
          data-testid={`${testId}-retry`}
          onClick={onRetry}
          className="inline-flex flex-none items-center gap-1 rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        >
          <RefreshCw aria-hidden="true" className="h-3 w-3" />
          Retry
        </button>
      )}
    </div>
  );
}
