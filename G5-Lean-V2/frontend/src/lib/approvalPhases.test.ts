import { describe, expect, it } from "vitest";
import {
  definitionPhases,
  admissionBlockReason,
  APPROVALS_REQUIRED_BEFORE_MERGING,
  mergeAdmission,
  requirementCountsState,
  requirementPhase,
  requirementStatusCounts,
  requirementStatusText,
} from "./approvalPhases";
import type { OrgApprovalDefinitionDTO } from "./types";
import {
  approvalDefinition,
  approvalRequirement,
  approvalsState,
} from "../test/approvalFixtures";

const definition = approvalDefinition;
const entry = approvalRequirement;
const state = approvalsState;

describe("approvalPhases (g5.workbench.gates)", () => {
  it("reads a definition's phases in workbench order and defaults an old payload to IE", () => {
    expect(definitionPhases(definition({ phases: ["ie", "be"] }))).toEqual([
      "be",
      "ie",
    ]);
    expect(definitionPhases(definition({ phases: [] }))).toEqual([]);
    expect(
      definitionPhases({ phases: undefined } as Pick<
        OrgApprovalDefinitionDTO,
        "phases"
      >),
    ).toEqual(["ie"]);
  });

  it("reads an entry's phase and defaults a missing one to IE", () => {
    expect(requirementPhase(entry({ phase: "se" }))).toBe("se");
    expect(requirementPhase({})).toBe("ie");
    expect(requirementPhase({ phase: null })).toBe("ie");
  });

  it("counts each requirement once by its server status, never by approvers (g5.governance.approval.status)", () => {
    const value = state([
      entry({ phase: "be", state: "approved" }),
      entry({
        phase: "be",
        state: "pending",
        decisionLog: [
          {
            requestId: null,
            outcome: "approved",
            comment: null,
            summary: null,
            actorType: "human",
            actor: null,
            definitionVersion: 1,
            superseded: true,
            reviewedGitHeads: [],
            createdAt: "2026-09-01T00:00:00Z",
          },
        ],
      }),
      entry({ phase: "ie", state: "rejected" }),
    ]);
    const counts = requirementStatusCounts(value);
    expect(counts).toEqual({
      required: 3,
      approved: 1,
      rejected: 1,
      pending: 1,
    });
    expect(requirementStatusText(counts)).toBe(
      "1 approved, 1 rejected, 1 pending",
    );
    expect(requirementCountsState(counts)).toBe("rejected");
    expect(requirementCountsState(requirementStatusCounts(null))).toBe("none");
    expect(
      requirementCountsState(
        requirementStatusCounts(state([entry({ state: "approved" })])),
      ),
    ).toBe("approved");
  });

  it("names unsatisfied approvals with the one generic ship blocker and keeps other refusals (g5.workbench.ship-to-parent)", () => {
    const requirement = (state: "approved" | "pending" | "rejected") => ({
      definitionId: "d",
      definitionName: "Security review",
      definitionVersion: 1,
      state,
    });
    expect(
      admissionBlockReason({
        admitted: true,
        reason: null,
        requirements: [requirement("approved")],
      }),
    ).toBeNull();
    expect(
      admissionBlockReason({
        admitted: false,
        reason: "Approval required before merging: Security review (pending)",
        requirements: [requirement("pending")],
      }),
    ).toBe(APPROVALS_REQUIRED_BEFORE_MERGING);
    expect(APPROVALS_REQUIRED_BEFORE_MERGING).toBe(
      "Approval(s) required before merging",
    );
    expect(
      admissionBlockReason({
        admitted: false,
        reason: "Approval required before merging: Security review (pending)",
        requirements: [],
      }),
    ).toBe(APPROVALS_REQUIRED_BEFORE_MERGING);
    expect(
      admissionBlockReason({
        admitted: false,
        reason: "Change the merge target first.",
        requirements: [],
      }),
    ).toBe("Change the merge target first.");
  });

  it("exposes the server admission and never invents one", () => {
    expect(mergeAdmission(state([]))).toBeNull();
    expect(mergeAdmission(null)).toBeNull();
    const admission = {
      admitted: false,
      reason: "IE pending",
      requirements: [],
    };
    expect(mergeAdmission(state([], admission))).toEqual(admission);
  });
});
