// =============================================================================
// Workspace landing — lifecycle and approval summaries
// =============================================================================
// One status vocabulary for issues, linked issues, the G5 PR and each provider
// PR (ui g5.workbench.landing.status): the same state reads the same way
// everywhere, and a state that could not be established is Unavailable, never
// assumed Open. The approval summary (g5.workbench.landing.summary) names the
// workspace-wide approval state from the real projection without implying
// that one approved type, or an unavailable read, satisfies the requirements.
// =============================================================================

import type {
  ExternalIssueSummaryDTO,
  WorkspaceApprovalsStateDTO,
  WorkspacePullRequestProviderRepoStatus,
  WorkspacePullRequestStatus,
} from "./types";
import {
  requirementCountsState,
  requirementStatusCounts,
  requirementStatusText,
} from "./approvalPhases";

export type LifecycleState =
  | "open"
  | "draft"
  | "merged"
  | "closed"
  | "unavailable";

/** A provider PR's lifecycle as GitHub reported it on the description read. */
export type ProviderLifecycle = "open" | "draft" | "closed" | "merged";

export const LIFECYCLE_LABEL: Record<LifecycleState, string> = {
  open: "Open",
  draft: "Draft",
  merged: "Merged",
  closed: "Closed",
  unavailable: "Unavailable",
};

/** An issue is Open or Closed; its close reason is never a landing label. */
export function issueLifecycle(
  issue: Pick<ExternalIssueSummaryDTO, "state"> | null | undefined,
): LifecycleState {
  if (issue?.state === "open") return "open";
  if (issue?.state === "closed") return "closed";
  return "unavailable";
}

/** The G5 PR's own lifecycle; it has no draft state. */
export function g5PullRequestLifecycle(
  status: WorkspacePullRequestStatus | string | null | undefined,
): LifecycleState {
  return status === "open" || status === "merged" || status === "closed"
    ? status
    : "unavailable";
}

/**
 * A provider PR's status: GitHub's live answer when there is one, else the
 * last recorded observation where it names a lifecycle. Closed and Draft
 * come only from GitHub; a failed or refused publication is no lifecycle.
 */
export function providerPullRequestLifecycle(
  recorded: WorkspacePullRequestProviderRepoStatus | string | null | undefined,
  live: ProviderLifecycle | null | undefined,
): LifecycleState {
  if (live) return live;
  if (recorded === "open" || recorded === "merged") return recorded;
  return "unavailable";
}

export type ApprovalSummaryKind =
  | "loading"
  | "unavailable"
  | "none-required"
  | "pending"
  | "rejected"
  | "approved";

export interface ApprovalSummary {
  kind: ApprovalSummaryKind;
  text: string;
}

/**
 * The workspace-wide approval state for the landing summary line
 * (g5.workbench.landing.summary): `N approved, M rejected, K pending`,
 * counting the outgoing requirements once each by their one server status
 * (g5.governance.approval.status), never reviewers. `status` is the approvals
 * projection's read state; an unreadable projection is Unavailable, never
 * approved. Requests no longer need a PR, so the PR's state does not change
 * what the line says.
 */
export function approvalSummary(
  state: WorkspaceApprovalsStateDTO | null | undefined,
  status: "ok" | "loading" | "error",
): ApprovalSummary {
  if (status === "loading") return { kind: "loading", text: "Loading…" };
  if (status === "error" || !state) {
    return { kind: "unavailable", text: "Unavailable" };
  }
  const counts = requirementStatusCounts(state);
  const overall = requirementCountsState(counts);
  if (overall === "none") {
    return { kind: "none-required", text: "No approvals required" };
  }
  return { kind: overall, text: requirementStatusText(counts) };
}
