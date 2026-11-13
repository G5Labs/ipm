import { describe, expect, it } from "vitest";
import {
  approvalSummary,
  g5PullRequestLifecycle,
  issueLifecycle,
  providerPullRequestLifecycle,
} from "./landingStatus";
import type {
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingRequirementDTO,
} from "./types";

function requirement(
  state: "approved" | "rejected" | "pending",
  approver: "approved" | "rejected" | "pending" | "not_requested",
): WorkspaceOutgoingRequirementDTO {
  return {
    definition: { id: `def-${Math.random()}`, name: "Review" },
    state,
    approvers: [
      {
        userId: "u",
        displayName: "U",
        email: "u@example.com",
        stillMember: true,
        state: approver,
        comment: null,
        decidedAt: null,
      },
    ],
    callerCanDecide: false,
    decisionLog: [],
  } as unknown as WorkspaceOutgoingRequirementDTO;
}

function state(
  requirements: WorkspaceOutgoingRequirementDTO[],
): WorkspaceApprovalsStateDTO {
  return {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "t",
      targetWorkspaceName: "t",
      requirements,
    },
  };
}

describe("lifecycle vocabulary", () => {
  it("maps issues, G5 PRs and provider PRs without assuming Open", () => {
    expect(issueLifecycle({ state: "open" })).toBe("open");
    expect(issueLifecycle({ state: "closed" })).toBe("closed");
    expect(issueLifecycle(null)).toBe("unavailable");
    expect(g5PullRequestLifecycle("merged")).toBe("merged");
    expect(g5PullRequestLifecycle(undefined)).toBe("unavailable");
    // GitHub's live answer wins; Closed and Draft come only from it.
    expect(providerPullRequestLifecycle("open", "draft")).toBe("draft");
    expect(providerPullRequestLifecycle("open", "closed")).toBe("closed");
    expect(providerPullRequestLifecycle("merged", null)).toBe("merged");
    expect(providerPullRequestLifecycle("action_required", null)).toBe(
      "unavailable",
    );
    expect(providerPullRequestLifecycle("failed", undefined)).toBe(
      "unavailable",
    );
  });
});

describe("approval summary", () => {
  it("counts requirements once each by their server status (g5.workbench.landing.summary)", () => {
    expect(approvalSummary(null, "loading").kind).toBe("loading");
    // Unreadable is unavailable, never approved.
    expect(approvalSummary(null, "error")).toEqual({
      kind: "unavailable",
      text: "Unavailable",
    });
    expect(approvalSummary(state([]), "ok")).toEqual({
      kind: "none-required",
      text: "No approvals required",
    });
    expect(
      approvalSummary(state([requirement("pending", "not_requested")]), "ok"),
    ).toEqual({ kind: "pending", text: "0 approved, 0 rejected, 1 pending" });
    expect(
      approvalSummary(
        state([
          requirement("approved", "approved"),
          requirement("pending", "pending"),
        ]),
        "ok",
      ),
    ).toEqual({ kind: "pending", text: "1 approved, 0 rejected, 1 pending" });
    expect(
      approvalSummary(state([requirement("approved", "approved")]), "ok"),
    ).toEqual({ kind: "approved", text: "1 approved, 0 rejected, 0 pending" });
    // An any-of whose agent rejected and a human approval superseded: the
    // server state is approved, so the rejected reviewer row is not counted.
    expect(
      approvalSummary(
        state([
          requirement("approved", "rejected"),
          requirement("approved", "approved"),
          requirement("rejected", "rejected"),
          requirement("pending", "approved"),
        ]),
        "ok",
      ),
    ).toEqual({ kind: "rejected", text: "2 approved, 1 rejected, 1 pending" });
  });
});
