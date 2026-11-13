import { LoaderCircle } from "lucide-react";
import type { WorkspaceIssueLinkDTO } from "../../lib/types";
import { describeIssueError } from "../../lib/externalIssueErrors";
import { Button } from "../ui/UI";
import { IssueErrorNotice } from "./ExternalIssueParts";

export function IssueLinkReadError({ link }: { link: WorkspaceIssueLinkDTO }) {
  return (
    <IssueErrorNotice
      compact
      testId={`workspace-issue-link-error-${link.id}`}
      presentation={describeIssueError(
        link.issueError?.issueError,
        link.issueError?.message,
      )}
    />
  );
}

export function IssueUnlinkConfirmation({
  link,
  unlinking,
  unlinkError,
  onCancel,
  onConfirm,
  warnPrimary = false,
}: {
  link: WorkspaceIssueLinkDTO;
  unlinking: boolean;
  unlinkError: unknown;
  onCancel: () => void;
  onConfirm: () => void;
  warnPrimary?: boolean;
}) {
  return (
    <div
      data-testid="workspace-issue-unlink-confirm"
      className="mt-2.5 flex flex-wrap items-center gap-2 rounded-lg border border-[var(--g5-blocked)]/20 bg-[var(--g5-blocked)]/5 px-3 py-2"
    >
      <p className="min-w-0 flex-1 text-xs text-[var(--g5-text1)]">
        Unlink {link.displayKey} from this workspace? The issue itself is not
        changed.
        {warnPrimary &&
          link.role === "primary" &&
          " This workspace will lose its primary issue."}
      </p>
      <Button
        kind="secondary"
        onClick={() => onCancel()}
        data-testid="workspace-issue-unlink-cancel"
        className="px-3 py-1 text-xs"
      >
        Cancel
      </Button>
      <Button
        kind="primary"
        disabled={unlinking}
        onClick={() => {
          onConfirm();
        }}
        data-testid="workspace-issue-unlink-confirm-button"
        className="px-3 py-1 text-xs"
      >
        {unlinking && <LoaderCircle className="h-3 w-3 animate-spin" />}
        Confirm unlink
      </Button>
      {unlinkError != null && (
        <p
          role="alert"
          data-testid="workspace-issue-unlink-error"
          className="w-full text-xs text-[var(--g5-blocked)]"
        >
          The issue could not be unlinked. Try again.
        </p>
      )}
    </div>
  );
}
