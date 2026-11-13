// =============================================================================
// Fork dialog — optional existing issue to link
// =============================================================================
// An issue URL, `owner/name#number` or native `ACME-12` in one of the
// organization's issue domains, and the role the new workspace takes for it. The server
// resolves the reference when the fork is admitted and links the issue when
// the workspace is published.
// =============================================================================

import { useId } from "react";

import { describeIssueRequestError } from "../../lib/externalIssueErrors";
import type {
  AsyncState,
  ExternalIssueSummaryDTO,
  WorkspaceIssueLinkRole,
} from "../../lib/types";

const ROLE_OPTIONS: Array<{
  value: WorkspaceIssueLinkRole;
  label: string;
}> = [
  { value: "primary", label: "Primary" },
  { value: "related", label: "Related" },
];

const FIELD_CLASSES =
  "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

export interface ForkIssueFieldProps {
  issueRef: string;
  /** Lookup of the current reference; `null` while none is current. */
  lookup: AsyncState<ExternalIssueSummaryDTO> | null;
  role: WorkspaceIssueLinkRole;
  onIssueRefChange: (value: string) => void;
  onRoleChange: (value: WorkspaceIssueLinkRole) => void;
}

export function ForkIssueField({
  issueRef,
  lookup,
  role,
  onIssueRefChange,
  onRoleChange,
}: ForkIssueFieldProps) {
  const inputId = useId();
  const roleId = useId();
  const hintId = useId();
  return (
    <div data-testid="clone-issue-field">
      <label
        htmlFor={inputId}
        className="mb-1 block text-sm font-medium text-[var(--g5-text1)]"
      >
        Issue (optional)
      </label>
      <div className="flex gap-2">
        <input
          id={inputId}
          type="text"
          data-testid="clone-issue-ref"
          aria-describedby={hintId}
          value={issueRef}
          onChange={(event) => onIssueRefChange(event.target.value)}
          placeholder="ACME-12, owner/repo#123 or issue URL"
          className={`min-w-0 flex-1 ${FIELD_CLASSES}`}
        />
        <label htmlFor={roleId} className="sr-only">
          Issue role
        </label>
        <select
          id={roleId}
          data-testid="clone-issue-role"
          value={role}
          disabled={issueRef.trim() === ""}
          onChange={(event) =>
            onRoleChange(event.target.value as WorkspaceIssueLinkRole)
          }
          className={`flex-none disabled:opacity-60 ${FIELD_CLASSES}`}
        >
          {ROLE_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>
      {lookup?.status === "loading" && (
        <p
          role="status"
          data-testid="clone-issue-lookup"
          className="mt-1 text-xs text-[var(--g5-text2)]"
        >
          Looking up issue…
        </p>
      )}
      {lookup?.status === "ok" && (
        <p
          data-testid="clone-issue-lookup"
          className="mt-1 truncate text-xs text-[var(--g5-text1)]"
        >
          <span className="font-mono">{lookup.data.displayKey}</span> ·{" "}
          {lookup.data.title}
        </p>
      )}
      {lookup?.status === "error" && (
        <p
          role="alert"
          data-testid="clone-issue-lookup-error"
          className="mt-1 text-xs text-[var(--g5-blocked)]"
        >
          {describeIssueRequestError(lookup.error).title}
        </p>
      )}
      <p id={hintId} className="mt-1 text-xs text-[var(--g5-text2)]">
        Links an existing issue to the new workspace. A primary issue belongs to
        one workspace at a time.
      </p>
    </div>
  );
}
