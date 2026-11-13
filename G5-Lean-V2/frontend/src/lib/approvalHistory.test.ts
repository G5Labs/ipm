import { describe, expect, it } from "vitest";

import {
  buildApprovalHistoryCards,
  resolveApprovalEvidence,
  resolveApprovalHistoryPresentation,
} from "./approvalHistory";
import type { AgentApprovalReviewDTO } from "./types";

describe("resolveApprovalHistoryPresentation", () => {
  const humanCases = [
    [null, null, null, null],
    ["pending", null, null, "Awaiting decision"],
    ["settled", null, "approved", "Approved"],
    ["settled", null, "rejected", "Rejected"],
    ["settled", null, "deferred", "Decision withdrawn"],
    ["cancelled", null, null, "Cancelled"],
    ["expired", null, null, "Expired"],
    ["superseded", null, null, "Superseded"],
    ["bypassed", null, null, "Bypassed by override"],
  ] as const;

  it.each(humanCases)(
    "maps the human ontology row request=%s run=%s decision=%s",
    (requestState, runState, decision, expected) => {
      expect(
        resolveApprovalHistoryPresentation(
          "human",
          requestState,
          runState,
          decision,
        )?.label ?? null,
      ).toBe(expected);
    },
  );

  const agentCases = [
    [null, null, null, null],
    ["pending", null, null, "Queued"],
    ["pending", "queued", null, "Waiting for agent"],
    ["pending", "running", null, "In progress"],
    ["pending", "waiting", null, "Waiting"],
    ["pending", "succeeded", null, "Review complete"],
    ["settled", "succeeded", "approved", "Approved"],
    ["settled", "succeeded", "rejected", "Rejected"],
    ["settled", "succeeded", "deferred", "No decision"],
    ["pending", "failed", null, "Failed"],
    ["pending", "cancelled", null, "Cancelled"],
    ["expired", null, null, "Expired"],
    ["superseded", null, null, "Superseded"],
    ["bypassed", null, null, "Bypassed by override"],
  ] as const;

  it.each(agentCases)(
    "maps the agent ontology row request=%s run=%s decision=%s",
    (requestState, runState, decision, expected) => {
      expect(
        resolveApprovalHistoryPresentation(
          "agent",
          requestState,
          runState,
          decision,
        )?.label ?? null,
      ).toBe(expected);
    },
  );

  it.each(["human", "agent"] as const)(
    "reads a %s request closed by a merge with override as bypassed, never as a decision",
    (approverType) => {
      expect(
        resolveApprovalHistoryPresentation(
          approverType,
          "bypassed",
          null,
          null,
        ),
      ).toEqual({
        label: "Bypassed by override",
        requestStatus: "Bypassed",
        decisionFilter: "no_decision",
        tone: "warning",
      });
    },
  );

  it("keeps the request pending after its review conversation completes", () => {
    expect(
      resolveApprovalHistoryPresentation("agent", "pending", "succeeded", null),
    ).toMatchObject({
      label: "Review complete",
      requestStatus: "Active",
      decisionFilter: "no_decision",
    });
  });

  it("renders an explicit diagnostic for an unmapped runtime combination", () => {
    expect(
      resolveApprovalHistoryPresentation(
        "agent",
        "pending",
        "future_runtime_state",
        null,
      ),
    ).toMatchObject({ label: "Unknown state", requestStatus: "Unknown" });
  });
});

describe("review conversations", () => {
  const review: AgentApprovalReviewDTO = {
    taskId: "task-1",
    sessionId: "session-1",
    state: "succeeded",
    terminalReason: null,
    createdAt: "2026-09-01T12:00:00Z",
    startedAt: "2026-09-01T12:01:00Z",
    updatedAt: "2026-09-01T12:04:00Z",
  };
  it("inspects failed formal attempts through retained data without linking a live private session", () => {
    expect(
      resolveApprovalEvidence(
        {
          ...review,
          reviewId: "formal-failed",
          reportAvailable: false,
          state: "failed",
        },
        "pending",
      ),
    ).toEqual({
      kind: "inspection",
      label: "Inspect review",
      reviewId: "formal-failed",
    });
  });
  it("links the review conversation without claiming a formal decision", () => {
    expect(resolveApprovalEvidence(review, "pending")).toEqual({
      kind: "conversation",
      label: "Open review conversation",
      sessionId: "session-1",
    });
    expect(
      resolveApprovalHistoryPresentation("agent", "pending", "succeeded", null),
    ).toMatchObject({
      label: "Review complete",
      decisionFilter: "no_decision",
    });
  });
  it("distinguishes an unstarted request from an unavailable conversation", () => {
    expect(resolveApprovalEvidence(null, "pending")).toEqual({
      kind: "pending",
      label: "Review not yet started",
    });
    expect(
      resolveApprovalEvidence({ ...review, sessionId: null }, "pending"),
    ).toEqual({
      kind: "unavailable",
      label: "Review conversation unavailable",
    });
  });
  it.each(["succeeded", "failed"] as const)(
    "records %s execution without fabricated publication or cleanup timestamps",
    (state) => {
      const currentReview = {
        ...review,
        state,
        endedAt: "2026-09-01T12:03:00Z",
        updatedAt: "2026-09-02T12:04:00Z",
      };
      const [card] = buildApprovalHistoryCards({
        requiredInto: [],
        attachmentHistory: [],
        outgoing: {
          targetWorkspaceId: "target",
          targetWorkspaceName: "Target",
          requirements: [],
          history: [
            {
              definitionId: "def",
              definitionName: "Review",
              decisionLog: [],
              requests: [
                {
                  requestId: "req",
                  actorType: "agent",
                  actorUserId: null,
                  actor: null,
                  requestedBy: null,
                  state: "pending",
                  definitionVersion: 1,
                  definitionName: "Review",
                  requestedAt: review.createdAt,
                  closedAt: null,
                  agentReviews: [currentReview],
                },
              ],
            },
          ],
        },
      });
      expect(card.presentation.decisionFilter).toBe("no_decision");
      expect(card.events.map((e) => e.label)).toContain("Review started");
      expect(card.events.map((e) => e.label)).not.toContain(
        "Decision finalized",
      );
      if (state === "failed") {
        expect(
          card.events.find((event) => event.label === "Review failed")?.at,
        ).toBe(currentReview.endedAt);
      }
    },
  );
});

it("binds historical decisions only to their recorded run and retains every attempt event", () => {
  const attempt = (id: string): AgentApprovalReviewDTO => ({
    reviewId: id,
    taskId: id,
    sessionId: null,
    state: "succeeded",
    reportAvailable: true,
    terminalReason: null,
    createdAt: "2026-10-21T00:00:00Z",
    startedAt: null,
    updatedAt: "2026-10-21T00:00:00Z",
  });
  const decision = {
    requestId: "request",
    agentReviewId: "recorded",
    actorType: "agent" as const,
    actor: null,
    outcome: "approved" as const,
    comment: null,
    superseded: false,
    definitionVersion: 1,
    reviewedGitHeads: [],
    createdAt: "2026-10-21T00:00:00Z",
  };
  const state = {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "target",
      targetWorkspaceName: "Target",
      requirements: [],
      history: [
        {
          definitionId: "definition",
          definitionName: "Definition",
          decisionLog: [decision],
          requests: [
            {
              requestId: "request",
              definitionName: "Definition",
              definitionVersion: 1,
              actorType: "agent" as const,
              actorUserId: null,
              actor: null,
              requestedBy: null,
              state: "settled" as const,
              requestedAt: "2026-10-21T00:00:00Z",
              closedAt: "2026-10-21T00:00:00Z",
              agentReviews: [attempt("recorded"), attempt("different")],
            },
          ],
        },
      ],
    },
  };
  let [card] = buildApprovalHistoryCards(state);
  expect(card.review?.reviewId).toBe("recorded");
  expect(
    card.events.filter((e) => e.label === "Review run created"),
  ).toHaveLength(2);
  decision.agentReviewId = "missing";
  [card] = buildApprovalHistoryCards(state);
  expect(card.review).toBeNull();
  delete (decision as { agentReviewId?: string }).agentReviewId;
  [card] = buildApprovalHistoryCards(state);
  expect(card.review).toBeNull();
});
