// Home approvals count the workspace's required approval requirements once
// each by their one server status (g5.ui.home-workspaces,
// g5.governance.approval.status); approvers are never tallied. Phase
// associations recommend request timing and never partition merge admission.

import React from "react";
import {
  requirementCountsState,
  requirementStatusCounts,
  requirementStatusText,
  type RequirementStatusCounts,
} from "../../lib/approvalPhases";
import type { HomeApprovalsRead } from "./useHomeWorkspaceSignals";

export type ApprovalGateState = "ok" | "wait" | "rej" | "none";

export function approvalGateState(
  counts: RequirementStatusCounts,
): ApprovalGateState {
  const state = requirementCountsState(counts);
  return state === "none"
    ? "none"
    : state === "rejected"
      ? "rej"
      : state === "approved"
        ? "ok"
        : "wait";
}

export interface ApprovalGatesProps {
  read: HomeApprovalsRead;
  workspaceName: string;
}

export const ApprovalGates: React.FC<ApprovalGatesProps> = ({
  read,
  workspaceName,
}) => {
  if (read.status === "loading") {
    return (
      <span
        data-testid="home-workspace-approvals-pending"
        role="status"
        className="inline-flex items-center gap-0.5"
      >
        <span className="sr-only">Loading {workspaceName} approvals…</span>
        <span
          aria-hidden="true"
          className="inline-block h-[18px] w-24 animate-pulse rounded bg-[var(--g5-bg2)] motion-reduce:animate-none"
        />
      </span>
    );
  }
  if (read.status === "error") {
    return (
      <span
        data-testid="home-workspace-approvals-error"
        className="inline-flex items-center gap-1 whitespace-nowrap text-[11px] text-[var(--g5-text2)]"
      >
        <span>Unavailable</span>
        <button
          type="button"
          data-testid="home-workspace-approvals-retry"
          aria-label={`Retry loading ${workspaceName} approvals`}
          className="underline hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          onClick={read.retry}
        >
          Retry
        </button>
      </span>
    );
  }
  const counts = requirementStatusCounts(read.state);
  if (counts.required === 0) {
    return <span data-testid="home-workspace-approvals-none" />;
  }
  const summary = requirementStatusText(counts);
  // Words carry the status; the theme foreground keeps them readable in
  // every theme (no status colour on text).
  return (
    <span
      data-testid="home-workspace-approvals"
      data-state={approvalGateState(counts)}
      role="img"
      aria-label={`${workspaceName} approvals: ${summary}`}
      title={summary}
      className="grid gap-px whitespace-nowrap text-[11px] leading-[1.35] tabular-nums text-[var(--g5-text0)]"
    >
      <span aria-hidden="true">{counts.approved} approved</span>
      <span aria-hidden="true">{counts.rejected} rejected</span>
      <span aria-hidden="true">{counts.pending} pending</span>
    </span>
  );
};
