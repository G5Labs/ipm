import { describe, expect, it } from "vitest";
import { personalApprovals, personalApprovalsTitle } from "./personalApprovals";
import type {
  ApprovalRequestDTO,
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingRequirementDTO,
} from "./types";

const me = "user-me";

function request(over: Partial<ApprovalRequestDTO> = {}): ApprovalRequestDTO {
  return {
    requestId: "r1",
    actorType: "human",
    actorUserId: me,
    actor: { userId: me, displayName: "Me" },
    requestedBy: { userId: "owner", displayName: "Maya Chen" },
    state: "pending",
    definitionVersion: 1,
    definitionName: "X",
    requestedAt: "2026-11-05T10:00:00Z",
    closedAt: null,
    agentReviews: [],
    ...over,
  };
}

function requirement(
  name: string,
  callerCanDecide: boolean,
  state: WorkspaceOutgoingRequirementDTO["state"] = "pending",
  requests: ApprovalRequestDTO[] = [request()],
): WorkspaceOutgoingRequirementDTO {
  return {
    definition: { name } as WorkspaceOutgoingRequirementDTO["definition"],
    state,
    approvers: [],
    callerCanDecide,
    requests,
    decisionLog: [],
  };
}

function loaded(
  requirements: WorkspaceOutgoingRequirementDTO[],
  refreshError: Error | null = null,
) {
  return {
    status: "ok" as const,
    refreshError,
    data: {
      outgoing: { requirements },
    } as unknown as WorkspaceApprovalsStateDTO,
  };
}

describe("personalApprovals", () => {
  it("lists every caller-decidable requirement, even one already approved", () => {
    expect(
      personalApprovals(
        loaded([
          requirement("Security", true, "approved"),
          requirement("Architecture", true),
          requirement("UX", false),
        ]),
        me,
      ),
    ).toEqual({
      kind: "requested",
      names: ["Architecture", "Security"],
      requester: "Maya Chen",
    });
  });

  it("names a requester only when one person asked for every review", () => {
    const state = personalApprovals(
      loaded([
        requirement("A", true),
        requirement("B", true, "pending", [
          request({ requestedBy: { userId: "o2", displayName: "Sam" } }),
        ]),
      ]),
      me,
    );
    expect(state).toMatchObject({ kind: "requested", requester: null });
    expect(
      personalApprovals(
        loaded([
          requirement("A", true, "pending", [request({ requestedBy: null })]),
        ]),
        me,
      ),
    ).toMatchObject({ requester: null });
  });

  it("separates known zero from loading and unreadable reads", () => {
    expect(personalApprovals(loaded([requirement("A", false)]), me)).toEqual({
      kind: "none",
    });
    expect(
      personalApprovals(
        { status: "ok", refreshError: null, data: { outgoing: null } as never },
        me,
      ),
    ).toEqual({ kind: "none" });
    expect(personalApprovals({ status: "loading" }, me)).toEqual({
      kind: "loading",
    });
    expect(
      personalApprovals({ status: "error", error: new Error("x") }, me),
    ).toEqual({ kind: "unavailable" });
    expect(
      personalApprovals(loaded([requirement("A", true)], new Error("x")), me),
    ).toEqual({ kind: "unavailable" });
  });

  it("words the count in singular and plural", () => {
    expect(personalApprovalsTitle(1)).toBe("1 approval requested from you");
    expect(personalApprovalsTitle(6)).toBe("6 approvals requested from you");
  });
});
