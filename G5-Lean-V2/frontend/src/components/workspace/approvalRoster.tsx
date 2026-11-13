// =============================================================================
// G5 Workspace — approval roster helpers
// =============================================================================
// The avatar the Approvals page's configuration section renders for each
// configured approver (g5.workbench.gates). Requirement status comes from the
// server (g5.governance.approval.status), never from these entries.
// =============================================================================

import { Bot, Check, X } from "lucide-react";

import { InitialsAvatar } from "../org/OrgApprovalsPageContent";
import type {
  ApprovalRosterEntry,
  ApprovalRosterEntryState,
} from "../../lib/approvalPhases";

export type ShipApprovalEntryState = ApprovalRosterEntryState;

/** One cluster/popover entry — a distinct human, or one agent per requirement. */
export type ShipApprovalRosterEntry = ApprovalRosterEntry;

const ENTRY_RING: Record<ShipApprovalEntryState, string> = {
  approved: "0 0 0 1.5px var(--g5-ok)",
  rejected: "0 0 0 1.5px var(--g5-blocked)",
  pending: "0 0 0 1.5px color-mix(in srgb, var(--g5-text2) 45%, transparent)",
};

function rosterEntryTitle(entry: ShipApprovalRosterEntry): string {
  return `${entry.displayName} · ${entry.state} · ${entry.definitionNames.join(", ")}`;
}

function formatDecisionDate(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function RosterAvatar({
  entry,
  first,
}: {
  entry: ShipApprovalRosterEntry;
  first: boolean;
}) {
  return (
    <span
      title={rosterEntryTitle(entry)}
      className={`relative inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-full border-2 border-[var(--g5-bg1)] transition-[margin,opacity] duration-150 ${
        first ? "" : "-ml-2 group-hover:ml-0.5"
      } ${entry.state === "pending" ? "opacity-45 group-hover:opacity-80" : ""}`}
      style={{ boxShadow: ENTRY_RING[entry.state] }}
    >
      {entry.kind === "human" && entry.userId ? (
        <InitialsAvatar
          userId={entry.userId}
          displayName={entry.displayName}
          size="h-full w-full text-[9px]"
          title=""
        />
      ) : (
        <span className="flex h-full w-full items-center justify-center rounded-full border border-[rgb(192,132,252,0.3)] bg-[rgb(192,132,252,0.15)]">
          <Bot
            size={13}
            className="text-[rgb(206,160,252)]"
            aria-hidden="true"
          />
        </span>
      )}
      {entry.state === "approved" ? (
        <span className="absolute -bottom-0.5 -right-1 flex h-[11px] w-[11px] items-center justify-center rounded-full border border-[var(--g5-bg1)] bg-[var(--g5-ok)]">
          <Check
            size={7}
            strokeWidth={4}
            className="text-[rgb(8,12,24)]"
            aria-hidden="true"
          />
        </span>
      ) : null}
      {entry.state === "rejected" ? (
        <span className="absolute -bottom-0.5 -right-1 flex h-[11px] w-[11px] items-center justify-center rounded-full border border-[var(--g5-bg1)] bg-[var(--g5-blocked)]">
          <X
            size={7}
            strokeWidth={4}
            className="text-[rgb(8,12,24)]"
            aria-hidden="true"
          />
        </span>
      ) : null}
    </span>
  );
}
