import type {
  AgentApprovalReviewDTO,
  ApprovalDecisionLogEntryDTO,
  ApprovalRequestDTO,
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingHistoryDTO,
} from "./types";

export type ApprovalHistoryLabel =
  | "Awaiting decision"
  | "Queued"
  | "Waiting for agent"
  | "Waiting"
  | "Review complete"
  | "In progress"
  | "Publishing evidence"
  | "Recording decision"
  | "Approved"
  | "Rejected"
  | "Decision withdrawn"
  | "No decision"
  | "Failed"
  | "Cancelled"
  | "Expired"
  | "Superseded"
  | "Bypassed by override"
  | "Unknown state";

export type ApprovalHistoryRequestStatus =
  | "Active"
  | "Settled"
  | "Failed"
  | "Cancelled"
  | "Expired"
  | "Superseded"
  | "Bypassed"
  | "Unknown";

export type ApprovalHistoryDecisionFilter =
  | "approved"
  | "rejected"
  | "no_decision";

export interface ApprovalHistoryPresentation {
  label: ApprovalHistoryLabel;
  requestStatus: ApprovalHistoryRequestStatus;
  decisionFilter: ApprovalHistoryDecisionFilter;
  tone: "success" | "danger" | "warning" | "info" | "neutral";
}

/**
 * Closed request-card mapping from workspace-approvals-management D15.
 * Strings are accepted deliberately so a future wire value reaches the
 * explicit Unknown state instead of being displayed as a known state.
 */
export function resolveApprovalHistoryPresentation(
  approverType: string,
  requestState: string | null,
  runState: string | null,
  decisionOutcome: string | null,
): ApprovalHistoryPresentation | null {
  if (requestState === null) return null;

  if (approverType === "human") {
    if (requestState === "pending" && decisionOutcome === null) {
      return presentation(
        "Awaiting decision",
        "Active",
        "no_decision",
        "warning",
      );
    }
    if (requestState === "settled") {
      if (decisionOutcome === "approved") {
        return presentation("Approved", "Settled", "approved", "success");
      }
      if (decisionOutcome === "rejected") {
        return presentation("Rejected", "Settled", "rejected", "danger");
      }
      if (decisionOutcome === "deferred") {
        return presentation(
          "Decision withdrawn",
          "Settled",
          "no_decision",
          "neutral",
        );
      }
      return unknownPresentation();
    }
    if (decisionOutcome === null) {
      const terminal = terminalRequestPresentation(requestState);
      if (terminal) return terminal;
    }
    return unknownPresentation();
  }

  if (approverType === "agent") {
    if (requestState === "settled") {
      if (decisionOutcome === "approved") {
        return presentation("Approved", "Settled", "approved", "success");
      }
      if (decisionOutcome === "rejected") {
        return presentation("Rejected", "Settled", "rejected", "danger");
      }
      if (decisionOutcome === null || decisionOutcome === "deferred") {
        return presentation("No decision", "Settled", "no_decision", "neutral");
      }
      return unknownPresentation();
    }

    if (requestState === "pending" && decisionOutcome === null) {
      switch (runState) {
        case null:
          return presentation("Queued", "Active", "no_decision", "warning");
        case "queued":
          return presentation(
            "Waiting for agent",
            "Active",
            "no_decision",
            "warning",
          );
        case "running":
          return presentation("In progress", "Active", "no_decision", "info");
        case "waiting":
          return presentation("Waiting", "Active", "no_decision", "warning");
        case "succeeded":
          return presentation(
            "Review complete",
            "Active",
            "no_decision",
            "neutral",
          );
        case "failed":
          return presentation("Failed", "Failed", "no_decision", "danger");
        case "cancelled":
          return presentation(
            "Cancelled",
            "Cancelled",
            "no_decision",
            "neutral",
          );
        default:
          return unknownPresentation();
      }
    }

    if (decisionOutcome === null) {
      const terminal = terminalRequestPresentation(requestState);
      if (terminal) return terminal;
    }
    return unknownPresentation();
  }

  return unknownPresentation();
}

function presentation(
  label: ApprovalHistoryLabel,
  requestStatus: ApprovalHistoryRequestStatus,
  decisionFilter: ApprovalHistoryDecisionFilter,
  tone: ApprovalHistoryPresentation["tone"],
): ApprovalHistoryPresentation {
  return { label, requestStatus, decisionFilter, tone };
}

function unknownPresentation(): ApprovalHistoryPresentation {
  return presentation("Unknown state", "Unknown", "no_decision", "danger");
}

function terminalRequestPresentation(
  requestState: string,
): ApprovalHistoryPresentation | null {
  switch (requestState) {
    case "cancelled":
      return presentation("Cancelled", "Cancelled", "no_decision", "neutral");
    case "expired":
      return presentation("Expired", "Expired", "no_decision", "neutral");
    case "superseded":
      return presentation("Superseded", "Superseded", "no_decision", "neutral");
    // A merge with override landed without this request's decision
    // (g5.governance.approval.request-status): never an approval.
    case "bypassed":
      return presentation(
        "Bypassed by override",
        "Bypassed",
        "no_decision",
        "warning",
      );
    default:
      return null;
  }
}

export type ApprovalEvidence =
  | {
      kind: "conversation";
      label: "Open review conversation";
      sessionId: string;
    }
  | { kind: "report"; label: "Read review report"; reviewId: string }
  | { kind: "inspection"; label: "Inspect review"; reviewId: string }
  | { kind: "pending"; label: "Review not yet started" }
  | { kind: "unavailable"; label: "Review conversation unavailable" };

export function resolveApprovalEvidence(
  review: AgentApprovalReviewDTO | null,
  requestState: string,
): ApprovalEvidence {
  if (review?.reviewId && review.reportAvailable)
    return {
      kind: "report",
      label: "Read review report",
      reviewId: review.reviewId,
    };
  if (review?.reviewId)
    return {
      kind: "inspection",
      label: "Inspect review",
      reviewId: review.reviewId,
    };
  if (review?.sessionId)
    return {
      kind: "conversation",
      label: "Open review conversation",
      sessionId: review.sessionId,
    };
  return !review && requestState === "pending"
    ? { kind: "pending", label: "Review not yet started" }
    : { kind: "unavailable", label: "Review conversation unavailable" };
}

export interface ApprovalHistoryAttribute {
  label: string;
  value: string;
}

export interface ApprovalHistoryEvent {
  key: string;
  label: string;
  at: string;
  attributes: ApprovalHistoryAttribute[];
}

export interface ApprovalHistoryCard {
  key: string;
  approverType: "human" | "agent";
  approverKey: string;
  approverName: string;
  approvalKey: string;
  definitionName: string;
  request: ApprovalRequestDTO;
  review: AgentApprovalReviewDTO | null;
  decision: ApprovalDecisionLogEntryDTO | null;
  presentation: ApprovalHistoryPresentation;
  events: ApprovalHistoryEvent[];
}

export function buildApprovalHistoryCards(
  state: WorkspaceApprovalsStateDTO,
): ApprovalHistoryCard[] {
  const histories: WorkspaceOutgoingHistoryDTO[] =
    state.outgoing?.history ??
    (state.outgoing?.requirements ?? []).map((requirement) => ({
      definitionId: requirement.definition.id,
      definitionName: requirement.definition.name,
      requests: requirement.requests ?? [],
      decisionLog: requirement.decisionLog,
    }));
  const cards = new Map<string, ApprovalHistoryCard>();

  for (const history of histories) {
    for (const request of history.requests) {
      if (cards.has(request.requestId)) continue;
      const decision = decisionForRequest(request, history.decisionLog);
      const review =
        decision && request.actorType === "agent"
          ? decision.agentReviewId
            ? (request.agentReviews.find(
                (r) => r.reviewId === decision.agentReviewId,
              ) ?? null)
            : null
          : reviewForRequest(request);
      const resolved = resolveApprovalHistoryPresentation(
        request.actorType,
        request.state,
        review?.state ?? null,
        decision?.outcome ?? null,
      );
      if (!resolved) continue;
      const agent = request.actorType === "agent";
      cards.set(request.requestId, {
        key: request.requestId,
        approverType: request.actorType,
        approverKey: agent
          ? `agent:${history.definitionId}`
          : `user:${request.actorUserId ?? "unknown"}`,
        approverName: agent
          ? `${request.definitionName} agent`
          : (request.actor?.displayName ?? "Unknown approver"),
        approvalKey: history.definitionId,
        definitionName: history.definitionName,
        request,
        review,
        decision,
        presentation: resolved,
        events: buildTechnicalEvents(request, decision),
      });
    }
  }

  return Array.from(cards.values()).sort(
    (left, right) =>
      new Date(right.request.requestedAt).getTime() -
      new Date(left.request.requestedAt).getTime(),
  );
}

export type ApprovalTimelineEntry =
  | (ApprovalHistoryCard & { kind: "request"; at: string })
  | {
      kind: "decision";
      key: string;
      at: string;
      approvalKey: string;
      definitionName: string;
      approverKey: string;
      approverName: string;
      request: ApprovalRequestDTO | null;
      decision: ApprovalDecisionLogEntryDTO;
      presentation: ApprovalHistoryPresentation;
    };

/** Retain decisions that cannot be represented by a request's latest receipt. */
export function buildApprovalTimeline(
  state: WorkspaceApprovalsStateDTO,
  definitionId: string,
): ApprovalTimelineEntry[] {
  const cards = buildApprovalHistoryCards(state).filter(
    (card) => card.approvalKey === definitionId,
  );
  const entries: ApprovalTimelineEntry[] = cards.map((card) => ({
    ...card,
    kind: "request",
    at: card.events[0]?.at ?? card.request.requestedAt,
  }));
  const history = state.outgoing?.history?.find(
    (record) => record.definitionId === definitionId,
  );
  const requirement = state.outgoing?.requirements.find(
    (record) => record.definition.id === definitionId,
  );
  const decisions = history?.decisionLog ?? requirement?.decisionLog ?? [];
  // Request cards keep the original decision objects from this DTO snapshot.
  const represented = new Set(cards.map((card) => card.decision));
  const occurrences = new Map<string, number>();
  for (const decision of decisions) {
    if (represented.has(decision)) continue;
    const requestCard = cards.find(
      (card) => card.request.requestId === decision.requestId,
    );
    const agent = decision.actorType === "agent";
    const approved = decision.outcome === "approved";
    const rejected = decision.outcome === "rejected";
    const key = JSON.stringify([
      definitionId,
      decision.requestId,
      decision.agentReviewId,
      decision.actorType,
      decision.actor?.userId,
      decision.createdAt,
      decision.outcome,
      decision.definitionVersion,
      decision.comment,
      decision.summary,
      decision.reviewedGitHeads,
    ]);
    const occurrence = occurrences.get(key) ?? 0;
    occurrences.set(key, occurrence + 1);
    entries.push({
      kind: "decision",
      key: `decision:${key}:${occurrence}`,
      at: decision.createdAt,
      approvalKey: definitionId,
      definitionName:
        history?.definitionName ??
        requirement?.definition.name ??
        "Unknown approval",
      approverKey: agent
        ? `agent:${definitionId}`
        : `user:${decision.actor?.userId ?? "unknown"}`,
      approverName: agent
        ? `${history?.definitionName ?? requirement?.definition.name ?? "Unknown approval"} agent`
        : (decision.actor?.displayName ?? "Unknown approver"),
      request: requestCard?.request ?? null,
      decision,
      presentation: presentation(
        approved ? "Approved" : rejected ? "Rejected" : "Decision withdrawn",
        requestCard?.presentation.requestStatus ?? "Unknown",
        approved ? "approved" : rejected ? "rejected" : "no_decision",
        approved ? "success" : rejected ? "danger" : "neutral",
      ),
    });
  }
  return entries.sort(
    (left, right) => new Date(right.at).getTime() - new Date(left.at).getTime(),
  );
}

function decisionForRequest(
  request: ApprovalRequestDTO,
  decisions: ApprovalDecisionLogEntryDTO[],
): ApprovalDecisionLogEntryDTO | null {
  const exact = decisions.find(
    (decision) => decision.requestId === request.requestId,
  );
  if (exact) return exact;
  return null;
}

function reviewForRequest(
  request: ApprovalRequestDTO,
): AgentApprovalReviewDTO | null {
  return request.agentReviews.at(-1) ?? null;
}

function buildTechnicalEvents(
  request: ApprovalRequestDTO,
  decision: ApprovalDecisionLogEntryDTO | null,
): ApprovalHistoryEvent[] {
  const events: ApprovalHistoryEvent[] = [
    {
      key: `${request.requestId}:requested`,
      label: "Review requested",
      at: request.requestedAt,
      attributes: [
        {
          label: "Requested by",
          value: request.requestedBy?.displayName ?? "Unknown user",
        },
      ],
    },
  ];

  for (const review of request.agentReviews) {
    const runId = review.reviewId ?? review.taskId ?? review.createdAt;
    events.push({
      key: `${request.requestId}:${runId}:created`,
      label: "Review run created",
      at: review.createdAt,
      attributes: [
        {
          label: "Run ID",
          value: review.taskId ?? "Unavailable",
        },
      ],
    });
    if (review.startedAt) {
      events.push({
        key: `${request.requestId}:${runId}:assigned`,
        label: "Review started",
        at: review.startedAt,
        attributes: review.taskId
          ? [{ label: "Task ID", value: review.taskId }]
          : [],
      });
    }
    if (["failed", "cancelled"].includes(review.state)) {
      events.push({
        key: `${request.requestId}:${runId}:${review.state}`,
        label: review.state === "failed" ? "Review failed" : "Review cancelled",
        at: review.endedAt ?? review.updatedAt,
        attributes: [{ label: "Run state", value: review.state }],
      });
    }
  }

  if (decision) {
    events.push({
      key: `${request.requestId}:decision:${decision.createdAt}`,
      label: "Decision finalized",
      at: decision.createdAt,
      attributes: [
        {
          label: "Decision",
          value:
            decision.outcome === "approved"
              ? "Approved"
              : decision.outcome === "rejected"
                ? "Rejected"
                : "No decision",
        },
      ],
    });
  }

  if (request.closedAt) {
    events.push({
      key: `${request.requestId}:closed`,
      label: "Request closed",
      at: request.closedAt,
      attributes: [{ label: "Request status", value: request.state }],
    });
  }

  return events.sort(
    (left, right) => new Date(right.at).getTime() - new Date(left.at).getTime(),
  );
}
