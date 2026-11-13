// =============================================================================
// Typed 409 of a workspace issue link, with the "related" fallback
// =============================================================================
// Shared by the Workspace Issues panel and the Home Issues tile so both
// surfaces explain a refused link the same way: an already-linked issue is
// final; a workspace that already has a primary, or an issue another workspace
// owns as primary, may still be linked as related.
// =============================================================================

import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import type { LinkConflict } from "../../lib/externalIssueErrors";
import { Button } from "../ui/UI";

export interface IssueLinkConflictNoticeProps {
  conflict: LinkConflict;
  orgSlug: string;
  onLinkAsRelated: () => void;
  /** A link request is in flight: the fallback waits for it. */
  pending?: boolean;
  /**
   * Test id prefix: `<prefix>-conflict-<code>`, `<prefix>-conflict-owner`,
   * `<prefix>-link-as-related`.
   */
  testIdPrefix?: string;
  className?: string;
}

export function IssueLinkConflictNotice({
  conflict,
  orgSlug,
  onLinkAsRelated,
  pending = false,
  testIdPrefix = "workspace-issue",
  className = "mt-3",
}: IssueLinkConflictNoticeProps) {
  let message: ReactNode;
  let offerRelated = true;
  switch (conflict.code) {
    case "already_linked":
      message = "This issue is already linked to this workspace.";
      offerRelated = false;
      break;
    case "workspace_has_primary":
      message =
        "This workspace already has a primary issue. Link this issue as related, or unlink the current primary first.";
      break;
    case "issue_has_primary":
      message =
        conflict.ownerWorkspace?.visible === true ? (
          <>
            This issue is already the primary issue of workspace{" "}
            <Link
              to={`/${orgSlug}/${encodeURIComponent(conflict.ownerWorkspace.name)}?view=issues`}
              data-testid={`${testIdPrefix}-conflict-owner`}
              className="font-medium text-[var(--g5-info)] underline"
            >
              {conflict.ownerWorkspace.name}
            </Link>
            .
          </>
        ) : (
          "This issue is already the primary issue of another workspace you cannot access."
        );
      break;
  }
  return (
    <div
      role="alert"
      data-testid={`${testIdPrefix}-conflict-${conflict.code}`}
      className={`${className} flex flex-wrap items-center gap-3 rounded-lg border border-[rgb(242,162,73,0.35)] bg-[rgb(242,162,73,0.08)] px-3.5 py-2.5 text-sm text-[var(--g5-text1)]`}
    >
      <p className="min-w-0 flex-1">{message}</p>
      {offerRelated && (
        <Button
          kind="secondary"
          type="button"
          disabled={pending}
          onClick={onLinkAsRelated}
          data-testid={`${testIdPrefix}-link-as-related`}
          className="px-3 py-1 text-xs"
        >
          Link as related instead
        </Button>
      )}
    </div>
  );
}
