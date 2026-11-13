import { describe, expect, it } from "vitest";
import { buildApprovalTimeline } from "./approvalHistory";
import type {
  ApprovalDecisionLogEntryDTO,
  ApprovalRequestDTO,
  WorkspaceApprovalsStateDTO,
} from "./types";

const decision: ApprovalDecisionLogEntryDTO = {
  requestId: null,
  actorType: "human",
  actor: { userId: "reader", displayName: "Reviewer" },
  outcome: "approved",
  comment: "Original decision",
  definitionVersion: 1,
  superseded: true,
  reviewedGitHeads: [],
  createdAt: "2026-09-01T12:00:00Z",
};
const request: ApprovalRequestDTO = {
  requestId: "request",
  actorType: "human",
  actorUserId: "reader",
  actor: { userId: "reader", displayName: "Reviewer" },
  requestedBy: { userId: "owner", displayName: "Owner" },
  state: "settled",
  definitionVersion: 1,
  definitionName: "Approval",
  requestedAt: "2026-09-01T10:00:00Z",
  closedAt: "2026-09-01T12:00:00Z",
  agentReviews: [],
};
function state(
  decisions: ApprovalDecisionLogEntryDTO[],
  requests: ApprovalRequestDTO[] = [],
): WorkspaceApprovalsStateDTO {
  return {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "parent",
      targetWorkspaceName: "Base",
      requirements: [],
      history: [
        {
          definitionId: "approval",
          definitionName: "Approval",
          requests,
          decisionLog: decisions,
        },
        {
          definitionId: "other",
          definitionName: "Approval",
          requests: [],
          decisionLog: [{ ...decision, comment: "Another definition" }],
        },
      ],
    },
  };
}

describe("complete approval timelines", () => {
  it("keeps distinct simultaneous decisions stable when another record arrives", () => {
    const other = {
      ...decision,
      comment: "Another retained decision at the same time",
    };
    const earlier = buildApprovalTimeline(state([decision, other]), "approval");
    const newer = buildApprovalTimeline(
      state([
        { ...decision, createdAt: "2026-10-01T12:00:00Z" },
        decision,
        other,
      ]),
      "approval",
    );
    expect(new Set(earlier.map((entry) => entry.key)).size).toBe(2);
    expect(
      newer
        .filter((entry) => entry.at === decision.createdAt)
        .map((entry) => entry.key),
    ).toEqual(earlier.map((entry) => entry.key));
  });
  it("retains an unlinked decision without inventing a request and isolates equal-named definitions", () => {
    const entries = buildApprovalTimeline(state([decision]), "approval");
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      kind: "decision",
      request: null,
      approvalKey: "approval",
      decision,
      presentation: { label: "Approved", requestStatus: "Unknown" },
    });
    expect(entries[0].decision).toBe(decision);
  });

  it("retains every decision on a request while representing its latest receipt only once", () => {
    const original = { ...decision, requestId: request.requestId };
    const withdrawn = {
      ...original,
      outcome: "deferred" as const,
      createdAt: "2026-09-01T13:00:00Z",
      comment: "Withdrawn",
    };
    const entries = buildApprovalTimeline(
      state([withdrawn, original], [request]),
      "approval",
    );
    expect(entries).toHaveLength(2);
    const receipt = entries.find((entry) => entry.kind === "request")!;
    const earlier = entries.find((entry) => entry.kind === "decision")!;
    expect(receipt.decision).toBe(withdrawn);
    expect(earlier.decision).toBe(original);
    expect(earlier.request).toBe(request);
    expect(earlier.presentation.requestStatus).toBe("Settled");
    expect(new Set(entries.map((entry) => entry.key)).size).toBe(2);
  });

  it("keeps earlier agent evidence bound to that decision's exact report", () => {
    const original = {
      ...decision,
      actorType: "agent" as const,
      actor: null,
      requestId: request.requestId,
      agentReviewId: "older-report",
    };
    const latest = {
      ...original,
      agentReviewId: "newer-report",
      outcome: "rejected" as const,
      createdAt: "2026-09-01T14:00:00Z",
    };
    const agentRequest = {
      ...request,
      actorType: "agent" as const,
      actor: null,
      actorUserId: null,
    };
    const entries = buildApprovalTimeline(
      state([latest, original], [agentRequest]),
      "approval",
    );
    expect(
      entries.find((entry) => entry.kind === "request")!.decision
        ?.agentReviewId,
    ).toBe("newer-report");
    expect(
      entries.find((entry) => entry.kind === "decision")!.decision
        ?.agentReviewId,
    ).toBe("older-report");
  });
});

it("orders entries by latest activity and each request's events newest first", () => {
  const oldRequest = {
    ...request,
    requestId: "old",
    requestedAt: "2026-09-01T09:00:00Z",
    closedAt: "2026-09-01T16:00:00Z",
  };
  const newRequest = {
    ...request,
    requestId: "new",
    requestedAt: "2026-09-01T14:00:00Z",
    closedAt: null,
    state: "pending" as const,
  };
  const entries = buildApprovalTimeline(
    state(
      [{ ...decision, requestId: "old", createdAt: "2026-09-01T16:00:00Z" }],
      [newRequest, oldRequest],
    ),
    "approval",
  );
  expect(entries.map((entry) => entry.request?.requestId)).toEqual([
    "old",
    "new",
  ]);
  const first = entries[0];
  if (first.kind !== "request") throw new Error("Missing request card");
  expect(first.events.map((event) => event.at)).toEqual(
    [...first.events.map((event) => event.at)].sort().reverse(),
  );
});
