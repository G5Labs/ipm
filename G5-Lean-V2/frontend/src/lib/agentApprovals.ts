import type { TimelineEntry } from "./types";

/**
 * Shared agent-approval staleness classification (chat_panel_runtime D25).
 *
 * A requested legacy approval is actionable only while its owning assistant
 * message is live. A native request is attempt-bound instead, so its liveness
 * follows the active task and exact attempt even though the workspace-VM path
 * does not persist an assistant placeholder until the final message. Both
 * render gating and composer gating use this function. These client gates are
 * UX only; the API rejects late or stale replies at the execution boundary.
 *
 * A request row delivered live, ahead of the history transaction that
 * validates and stores it (`metadata.live`, chat g5.chat.live-delivery), is
 * pending here like any other: it is the turn's open question, so the
 * composer stays gated. Whether it can be answered yet is the renderer's
 * decision, held until the stored row replaces it; a request the validation
 * rejects arrives as an error row with the same id and never becomes pending.
 */
export interface AgentApprovalClassification {
  /** Requested approvalIds that are resolved OR dead (interrupt / dead turn). */
  settledOrDead: Set<string>;
  /**
   * The dead subset of `settledOrDead`: requested, never resolved, and no
   * longer answerable. Render these as stopped history, not as answered.
   */
  dead: Set<string>;
  /** Definitive provider NACK/correlation failures for non-actionable error UI. */
  failed: Set<string>;
  /** Requested approvalIds still actionable, in request order. */
  pending: string[];
}

export interface ActiveApprovalTask {
  id: string;
  status: string;
  latestAttempt?: { attemptNumber: number } | null;
}

export function classifyAgentApprovals(
  entries: TimelineEntry[],
  messages: ReadonlyArray<{ id: string; role: string; status?: string }>,
  activeTask?: ActiveApprovalTask | null,
): AgentApprovalClassification {
  const settledOrDead = new Set<string>();
  const dead = new Set<string>();
  const failed = new Set<string>();
  const requestedAt = new Map<string, number>();
  // Legacy request liveness follows its owning assistant message. Native
  // request liveness follows the task/attempt identity collected below.
  const owningMessageId = new Map<string, string | undefined>();
  const nativeRequestTask = new Map<
    string,
    { taskId: string; attempt: number }
  >();
  let lastInterruptedIndex = -1;
  entries.forEach((entry, index) => {
    const providerEvent = entry.metadata?.providerEvent;
    // An interrupted turn terminates without a synthesized resolution:
    // approvals requested before it are dead.
    if (providerEvent === "provided_agent.interrupted") {
      lastInterruptedIndex = index;
      return;
    }
    const approvalId = entry.metadata?.approvalId;
    if (typeof approvalId !== "string") return;
    if (providerEvent === "provided_agent.approval.requested") {
      requestedAt.set(approvalId, index);
      const messageId = entry.metadata?.messageId;
      owningMessageId.set(
        approvalId,
        typeof messageId === "string" ? messageId : undefined,
      );
      const taskId = entry.metadata?.taskId;
      const attempt = entry.metadata?.attempt;
      const providerRequestId = entry.metadata?.providerRequestId;
      if (
        typeof taskId === "string" &&
        typeof attempt === "number" &&
        Number.isInteger(attempt) &&
        providerRequestId === approvalId
      ) {
        nativeRequestTask.set(approvalId, { taskId, attempt });
      }
    } else if (providerEvent === "provided_agent.approval.resolved") {
      settledOrDead.add(approvalId);
    } else if (providerEvent === "provided_agent.approval.failed") {
      settledOrDead.add(approvalId);
      dead.add(approvalId);
      failed.add(approvalId);
    }
  });
  // A question is actionable/composer-blocking ONLY while its OWN owning turn is
  // live (chat_panel_runtime D25 clause (b), issue #2318). Judge each approval by
  // the status of the assistant message that emitted it — the same owning-message
  // discriminant the message-block render path (clause (a)) already uses — so a
  // ghost approval owned by an earlier, already-terminal turn is dead even while a
  // later turn is still streaming.
  const liveMessageIds = new Set<string>();
  for (const message of messages) {
    if (
      message.role === "assistant" &&
      (message.status === "pending" || message.status === "streaming")
    ) {
      liveMessageIds.add(message.id);
    }
  }
  // A LEGACY/pre-deploy `provided_agent.approval.requested` entry carries no
  // `messageId` (the worker's `append_timeline` metadata is persisted unchanged),
  // so it cannot be judged per-owning-message. Those fall back to session-global
  // liveness — a genuinely-live legacy question must keep blocking the composer
  // rather than be wrongly marked dead (chat_panel_runtime D25, CodeRabbit review).
  const anyAssistantTurnLive = liveMessageIds.size > 0;
  const pending: string[] = [];
  for (const [approvalId, index] of requestedAt) {
    if (settledOrDead.has(approvalId)) continue;
    const ownerId = owningMessageId.get(approvalId);
    const nativeRequest = nativeRequestTask.get(approvalId);
    const nativeTaskLive = nativeRequest
      ? activeTask == null
        ? true
        : activeTask.id === nativeRequest.taskId &&
          activeTask.status === "running" &&
          (activeTask.latestAttempt?.attemptNumber === undefined ||
            activeTask.latestAttempt.attemptNumber === nativeRequest.attempt)
      : false;
    const owningTurnLive = nativeRequest
      ? nativeTaskLive
      : typeof ownerId === "string"
        ? liveMessageIds.has(ownerId)
        : anyAssistantTurnLive;
    if (index <= lastInterruptedIndex || !owningTurnLive) {
      // Dead once a later interrupt exists, or once the OWNING turn is no longer
      // live (its assistant message reached complete/stopped/error): there is no
      // one left to receive a decision.
      settledOrDead.add(approvalId);
      dead.add(approvalId);
    } else {
      pending.push(approvalId);
    }
  }
  return { settledOrDead, dead, failed, pending };
}

/**
 * Derived sub-states of an idle-timed-out AskUserQuestion
 * (chat_panel_runtime D25, plan `late_answerable_question` v1):
 *
 * - `timed_out_answerable` — settlement only: the shared question form stays
 *   fully actionable (identical to pending plus one hint line, Dismiss
 *   hidden) because the server admits a late answer as a new user turn.
 * - `late_answered` — a `provided_agent.approval.late_answered` marker entry
 *   exists: collapse to the Answered summary.
 * - `timed_out_superseded` — a newer user message or newer question request
 *   was created after the settlement: collapse to the non-actionable
 *   "Timed out — ask again" summary.
 */
export type TimedOutQuestionSubState =
  "timed_out_answerable" | "late_answered" | "timed_out_superseded";

/** Durable marker of an admitted late answer (chat_service D54). */
export const APPROVAL_LATE_ANSWERED_EVENT =
  "provided_agent.approval.late_answered";

/** Shared with ChatPanel's picker — single source for the tool identity. */
export const ASK_USER_QUESTION_TOOL_NAME = "AskUserQuestion";

interface TimelinePosition {
  createdAt: string;
  sequence: number;
}

function entryPosition(entry: TimelineEntry): TimelinePosition {
  return { createdAt: entry.createdAt, sequence: entry.sequence ?? 0 };
}

function positionAfter(a: TimelinePosition, b: TimelinePosition): boolean {
  const byTime = a.createdAt.localeCompare(b.createdAt);
  if (byTime !== 0) return byTime > 0;
  return a.sequence > b.sequence;
}

/**
 * Classify idle-timed-out AskUserQuestion approvals into their D25 sub-states
 * from durable timeline evidence only — an idle_timeout settlement, the
 * optional `late_answered` marker, and supersession evidence (a user chat
 * message or a newer question request created after the settlement). Because
 * every input is a persisted entry/message and the comparison is
 * order-independent, the classification survives both live updates and
 * reload (any entry arrival order). Non-question approvals and settlements
 * whose request entry is outside the loaded window are not classified; their
 * rendering keeps the plain timed-out collapse.
 *
 * These sub-states never feed composer gating: an idle_timeout settlement
 * settles its approval in `classifyAgentApprovals`, so no timed-out
 * sub-state re-blocks the composer.
 */
export function classifyTimedOutQuestions(
  entries: TimelineEntry[],
  messages: ReadonlyArray<{ role: string; createdAt: string }>,
): Map<string, TimedOutQuestionSubState> {
  const questionRequests = new Map<string, TimelinePosition>();
  const settlements = new Map<string, TimelinePosition>();
  const lateAnswered = new Set<string>();
  for (const entry of entries) {
    const providerEvent = entry.metadata?.providerEvent;
    const approvalId = entry.metadata?.approvalId;
    if (typeof approvalId !== "string") continue;
    if (
      providerEvent === "provided_agent.approval.requested" &&
      // The worker-stamped tag is the authoritative tool IDENTITY
      // (chat_panel_runtime D25): input shape alone must never classify an
      // entry as a question, and the server's late-answer admission gate is
      // tag-gated the same way — a tag-less entry is never late-answerable.
      entry.metadata?.toolName === ASK_USER_QUESTION_TOOL_NAME
    ) {
      questionRequests.set(approvalId, entryPosition(entry));
    } else if (
      providerEvent === "provided_agent.approval.resolved" &&
      entry.metadata?.resolutionSource === "idle_timeout"
    ) {
      const position = entryPosition(entry);
      const existing = settlements.get(approvalId);
      if (!existing || positionAfter(position, existing)) {
        settlements.set(approvalId, position);
      }
    } else if (providerEvent === APPROVAL_LATE_ANSWERED_EVENT) {
      lateAnswered.add(approvalId);
    }
  }
  const subStates = new Map<string, TimedOutQuestionSubState>();
  for (const [approvalId, settlement] of settlements) {
    if (!questionRequests.has(approvalId)) continue;
    // The marker wins over supersession evidence: the admitted late answer is
    // itself a user message created after the settlement, and it must render
    // as Answered, not as superseded.
    if (lateAnswered.has(approvalId)) {
      subStates.set(approvalId, "late_answered");
      continue;
    }
    let superseded = false;
    for (const [otherApprovalId, requestPosition] of questionRequests) {
      if (otherApprovalId === approvalId) continue;
      if (positionAfter(requestPosition, settlement)) {
        superseded = true;
        break;
      }
    }
    if (!superseded) {
      superseded = messages.some(
        (message) =>
          message.role === "user" &&
          message.createdAt.localeCompare(settlement.createdAt) > 0,
      );
    }
    subStates.set(
      approvalId,
      superseded ? "timed_out_superseded" : "timed_out_answerable",
    );
  }
  return subStates;
}
