// =============================================================================
// Test fixtures — approval definitions, requirement entries and the workspace
// approvals projection (g5.workbench.gates). Shared by the approval-phase
// helpers' tests and the Guided panel's.
// =============================================================================

import type {
  OrgApprovalDefinitionDTO,
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingRequirementDTO,
} from "../lib/types";

export function approvalDefinition(
  overrides: Partial<OrgApprovalDefinitionDTO> = {},
): OrgApprovalDefinitionDTO {
  return {
    id: "def-1",
    latestVersion: 1,
    name: "Sign-off",
    description: null,
    humans: [],
    agentType: null,
    agentPrompt: null,
    invalidatesPrevious: false,
    phases: ["ie"],
    createdBy: { userId: "u-1", displayName: "One" },
    createdAt: "2026-09-01T00:00:00Z",
    firstCreatedBy: { userId: "u-1", displayName: "One" },
    firstCreatedAt: "2026-09-01T00:00:00Z",
    ...overrides,
  };
}

export function approvalRequirement(
  overrides: Partial<WorkspaceOutgoingRequirementDTO> = {},
): WorkspaceOutgoingRequirementDTO {
  return {
    definition: approvalDefinition(),
    phase: "ie",
    state: "pending",
    approvers: [],
    callerCanDecide: false,
    decisionLog: [],
    ...overrides,
  };
}

export function approvalsState(
  requirements: WorkspaceOutgoingRequirementDTO[],
  mergeAdmission?: NonNullable<
    WorkspaceApprovalsStateDTO["outgoing"]
  >["mergeAdmission"],
): WorkspaceApprovalsStateDTO {
  return {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "parent-1",
      targetWorkspaceName: "parent",
      requirements,
      ...(mergeAdmission ? { mergeAdmission } : {}),
    },
  } as WorkspaceApprovalsStateDTO;
}

/** A human approver row as the projection reports it. */
export function approver(
  state: "approved" | "rejected" | "pending",
  overrides: Partial<WorkspaceOutgoingRequirementDTO["approvers"][number]> = {},
): WorkspaceOutgoingRequirementDTO["approvers"][number] {
  return {
    userId: "user-alex",
    displayName: "Alex Chen",
    email: "alex@g5labs.dev",
    stillMember: true,
    state,
    comment: null,
    decidedAt: state === "approved" ? "2026-09-01T10:00:00Z" : null,
    requestId: null,
    ...overrides,
  } as WorkspaceOutgoingRequirementDTO["approvers"][number];
}
