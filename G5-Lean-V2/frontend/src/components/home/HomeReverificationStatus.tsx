// =============================================================================
// Home — Base re-verification status line (ui g5.ui.home-reverification)
// =============================================================================
// One line of orange text above the tiles, not a warning and with no
// threshold, telling how much Base gained since it was last re-verified
// (ontology_and_harness g5.reverification.status), with a Re-run
// verification button that opens the dialog. The line shows only while the
// organization's verification switch is on and the status reads as enabled;
// an open dialog stays on the last status it saw, so a refusal such as the
// switch being turned off meanwhile is still shown there with its reason.
// The button is greyed with its reason for someone who cannot fork Base, and
// for everyone while a run is in progress.
// =============================================================================

import React, { useRef, useState } from "react";
import { Link } from "react-router-dom";

import { useBaseReverificationStatus } from "../../hooks/useBaseReverification";
import {
  reverificationStartBlockedReason,
  reverificationStatusSentence,
  type EnabledBaseReverificationStatus,
} from "../../lib/baseReverification";
import type { OrgMemberDTO } from "../../lib/types";
import { findOwner, memberLabel } from "../../lib/workspaceOwner";
import { workspacePagePath } from "../../lib/workspacePagePath";
import { classNames } from "../ui/UI";
import { HomeReverificationDialog } from "./HomeReverificationDialog";
import { HOME_ROW_ACTION_CLASSES } from "./HomeTileParts";

export interface HomeReverificationStatusProps {
  /** The organization's verification switch is exactly on. */
  switchOn: boolean;
  orgId: string;
  orgSlug: string;
  /** The organization roster, for the run workspace owner's name. */
  members: OrgMemberDTO[];
  /** Home's per-person page size, for the dialog's lists. */
  pageSize: number;
}

export function HomeReverificationStatus({
  switchOn,
  orgId,
  orgSlug,
  members,
  pageSize,
}: HomeReverificationStatusProps) {
  const statusQuery = useBaseReverificationStatus(orgId, switchOn);
  const [dialogOpen, setDialogOpen] = useState(false);
  const data = statusQuery.data;
  const status = switchOn && data?.enabled ? data : null;
  const lastEnabled = useRef<EnabledBaseReverificationStatus | null>(null);
  if (status) lastEnabled.current = status;
  const dialogStatus = dialogOpen ? lastEnabled.current : null;
  const dialog = dialogStatus && (
    <HomeReverificationDialog
      orgId={orgId}
      orgSlug={orgSlug}
      status={dialogStatus}
      members={members}
      pageSize={pageSize}
      onClose={() => setDialogOpen(false)}
    />
  );
  const blockedReason = status
    ? reverificationStartBlockedReason(status)
    : null;
  // The dialog sits beside the line, never inside it, so it keeps its state
  // when the line goes.
  return (
    <>
      {status && (
        <div
          data-testid="home-reverification"
          data-state={status.state}
          className="mb-4 flex flex-wrap items-center gap-x-3 gap-y-2"
        >
          <p
            data-testid="home-reverification-status"
            className="text-sm font-medium text-[var(--g5-warn)]"
          >
            <StatusSentence
              status={status}
              orgSlug={orgSlug}
              members={members}
            />
          </p>
          <button
            type="button"
            data-testid="home-reverification-open"
            aria-haspopup="dialog"
            disabled={!!blockedReason}
            // A greyed button's tooltip is its reason.
            title={blockedReason ?? undefined}
            onClick={() => setDialogOpen(true)}
            className={classNames(
              HOME_ROW_ACTION_CLASSES,
              "inline-flex h-6 items-center whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent",
            )}
          >
            Re-run verification
          </button>
        </div>
      )}
      {dialog}
    </>
  );
}

/** The line's text; in progress it names the run's workspace and its owner. */
function StatusSentence({
  status,
  orgSlug,
  members,
}: {
  status: EnabledBaseReverificationStatus;
  orgSlug: string;
  members: OrgMemberDTO[];
}) {
  const run = status.inProgress;
  if (status.state !== "in_progress" || !run) {
    return <>{reverificationStatusSentence(status)}</>;
  }
  const owner = findOwner(members, run.ownerUserId ?? run.startedBy);
  const ownerLabel = (owner && memberLabel(owner)) || "its owner";
  return (
    <>
      Verification in progress in{" "}
      {run.workspaceName && !run.workspaceDeleted ? (
        <>
          workspace{" "}
          <Link
            data-testid="home-reverification-workspace"
            to={workspacePagePath(orgSlug, run.workspaceName)}
            className="underline underline-offset-2 hover:no-underline"
          >
            {run.workspaceName}
          </Link>
        </>
      ) : (
        "a workspace being created"
      )}{" "}
      (by {ownerLabel})
    </>
  );
}
