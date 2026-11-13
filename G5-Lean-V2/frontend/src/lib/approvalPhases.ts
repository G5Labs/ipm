// Approval phase associations recommend when to request a workspace-wide review.
// They never gate phase movement or select which target requirements gate merge.
import type {
  OrgApprovalDefinitionDTO,
  WorkspaceApprovalsStateDTO,
  WorkspaceMergeAdmissionDTO,
  WorkspaceOutgoingRequirementDTO,
  WorkspacePhase,
} from "./types";
import {
  isWorkspacePhase,
  WORKBENCH_PHASES,
  WORKSPACE_PHASES,
} from "./workbenchDefaults";

export type { WorkspacePhase };
export { isWorkspacePhase };

/** The phases an approval can bind to or settle, in workbench order. */
export const APPROVAL_PHASES: readonly WorkspacePhase[] = WORKSPACE_PHASES;

/** Short label for chips and group headers (the workbench abbreviation). */
export function approvalPhaseLabel(phase: WorkspacePhase): string {
  return WORKBENCH_PHASES[phase].short;
}

/**
 * A definition's recommendation associations in workbench order. A missing `phases` on an
 * old cached payload reads as the server default (`["ie"]`).
 */
export function definitionPhases(
  definition: Pick<OrgApprovalDefinitionDTO, "phases">,
): WorkspacePhase[] {
  const bound = Array.isArray(definition.phases) ? definition.phases : null;
  if (bound === null) return ["ie"];
  return WORKSPACE_PHASES.filter((phase) => bound.includes(phase));
}

/** Current phase first, then remaining associations in engineering order. */
export function sortApprovalDefinitions<
  T extends Pick<OrgApprovalDefinitionDTO, "phases">,
>(definitions: readonly T[], current: WorkspacePhase): T[] {
  const order = [
    current,
    ...WORKSPACE_PHASES.filter((phase) => phase !== current),
  ];
  const rank = (definition: T) => {
    const index = order.findIndex((phase) =>
      definitionPhases(definition).includes(phase),
    );
    return index < 0 ? order.length : index;
  };
  return [...definitions].sort((a, b) => rank(a) - rank(b));
}

export function approvalRecommendation(
  definition: Pick<OrgApprovalDefinitionDTO, "phases">,
): string {
  return definitionPhases(definition)
    .map((phase) => `Recommended for ${WORKBENCH_PHASES[phase].name}`)
    .join(" · ");
}

/**
 * The phase an outgoing requirement entry projects. A missing `phase` on an
 * old cached payload reads as Integration Engineering.
 */
export function requirementPhase(entry: {
  phase?: WorkspacePhase | null;
}): WorkspacePhase {
  return isWorkspacePhase(entry.phase) ? entry.phase : "ie";
}

export type ApprovalRosterEntryState = "approved" | "rejected" | "pending";

export interface ApprovalRosterEntry {
  key: string;
  kind: "human" | "agent";
  displayName: string;
  userId: string | null;
  state: ApprovalRosterEntryState;
  /** Names of the requirements this entry must approve. */
  definitionNames: string[];
  /** Latest human decision timestamp, for the popover's Approved rows. */
  decidedAt: string | null;
}

/**
 * Required approval requirements counted once each by their one overall
 * status (g5.governance.approval.status). Never tallies approvers: the
 * server's `state` is the only status source.
 */
export interface RequirementStatusCounts {
  /** Every attached required requirement. */
  required: number;
  approved: number;
  rejected: number;
  pending: number;
}

export function requirementStatusCounts(
  state: WorkspaceApprovalsStateDTO | null | undefined,
): RequirementStatusCounts {
  return countRequirementStatuses(state?.outgoing?.requirements ?? []);
}

/** {@link requirementStatusCounts} over any list of requirement statuses. */
export function countRequirementStatuses(
  entries: readonly Pick<WorkspaceOutgoingRequirementDTO, "state">[],
): RequirementStatusCounts {
  const counts = {
    required: entries.length,
    approved: 0,
    rejected: 0,
    pending: 0,
  };
  for (const entry of entries) {
    if (entry.state === "approved") counts.approved += 1;
    else if (entry.state === "rejected") counts.rejected += 1;
    // Anything else, including an unknown future word, is not satisfied.
    else counts.pending += 1;
  }
  return counts;
}

/** `N approved, M rejected, K pending` (Home, landing, Prepare to Ship). */
export function requirementStatusText(counts: RequirementStatusCounts): string {
  return `${counts.approved} approved, ${counts.rejected} rejected, ${counts.pending} pending`;
}

/** The one overall word for a set of requirement counts. */
export function requirementCountsState(
  counts: RequirementStatusCounts,
): "none" | "approved" | "rejected" | "pending" {
  if (counts.required === 0) return "none";
  if (counts.rejected > 0) return "rejected";
  if (counts.approved === counts.required) return "approved";
  return "pending";
}

/**
 * The server's merge admission (all target requirements against the current cut).
 * Absent when the workspace has no merge target, or on an old cached
 * payload without the field — callers treat that as "unknown", never as
 * admitted.
 */
export function mergeAdmission(
  state: WorkspaceApprovalsStateDTO | null | undefined,
): WorkspaceMergeAdmissionDTO | null {
  const admission = state?.outgoing?.mergeAdmission;
  return admission && typeof admission.admitted === "boolean"
    ? admission
    : null;
}

/**
 * The exact Ship to parent blocker while required approvals are unsatisfied
 * (g5.workbench.ship-to-parent); it never repeats requirement names.
 */
export const APPROVALS_REQUIRED_BEFORE_MERGING =
  "Approval(s) required before merging";

/**
 * The ship blocker a refused admission adds: the generic approvals sentence
 * when any required requirement is unsatisfied, else the server's own
 * non-approval reason (such as a merge-target mismatch). Null when admitted.
 */
export function admissionBlockReason(
  admission: WorkspaceMergeAdmissionDTO,
): string | null {
  if (admission.admitted) return null;
  if (admission.requirements.some((entry) => entry.state !== "approved"))
    return APPROVALS_REQUIRED_BEFORE_MERGING;
  const reason = admission.reason?.trim();
  // A server that still names requirements in its approval reason reads the
  // same; a missing reason is an approvals refusal.
  return !reason || /^approval(s|\(s\))? required/i.test(reason)
    ? APPROVALS_REQUIRED_BEFORE_MERGING
    : reason;
}
