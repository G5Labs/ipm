import { describe, expect, it } from "vitest";

import { pendingAgentApprovalIdsForTimeline } from "./WorkspacePage";
import {
  classifyAgentApprovals,
  classifyTimedOutQuestions,
} from "../lib/agentApprovals";
import type { TimelineEntry } from "../lib/types";

function entry(id: string, metadata: Record<string, unknown>): TimelineEntry {
  return {
    id,
    entryType: "checkpoint",
    content: "",
    metadata: {
      source: "provided_agent",
      provider: "claude_code",
      ...metadata,
    },
    createdAt: "2026-05-10T00:00:00Z",
  };
}

// Every requested approval carries the id of the assistant message that emitted
// it (its owning turn). Liveness is judged PER approval from that message's
// status (issue #2318), so the fixtures pass a messages list, not a boolean.
const OWNER = "assistant-1";
const requested = (approvalId: string, messageId: string = OWNER) =>
  entry(`req-${approvalId}`, {
    providerEvent: "provided_agent.approval.requested",
    approvalId,
    messageId,
  });
const resolvedEntry = (approvalId: string) =>
  entry(`res-${approvalId}`, {
    providerEvent: "provided_agent.approval.resolved",
    approvalId,
  });
const failedEntry = (approvalId: string) =>
  entry(`failed-${approvalId}`, {
    providerEvent: "provided_agent.approval.failed",
    approvalId,
    errorCode: "UNKNOWN_APPROVAL",
  });
const interrupted = () =>
  entry("interrupted", { providerEvent: "provided_agent.interrupted" });

const assistantMsg = (id: string, status: string) => ({
  id,
  role: "assistant",
  status,
});
// The default owning turn, live vs terminal.
const liveOwner = [assistantMsg(OWNER, "streaming")];
const terminalOwner = [assistantMsg(OWNER, "complete")];

describe("pendingAgentApprovalIdsForTimeline", () => {
  it("reports an unresolved approval while its owning turn is live", () => {
    expect(
      pendingAgentApprovalIdsForTimeline([requested("a-1")], liveOwner),
    ).toEqual(["a-1"]);
  });

  it("drops resolved approvals", () => {
    expect(
      pendingAgentApprovalIdsForTimeline(
        [requested("a-1"), resolvedEntry("a-1")],
        liveOwner,
      ),
    ).toEqual([]);
  });

  it("drops definitively failed approval deliveries", () => {
    expect(
      pendingAgentApprovalIdsForTimeline(
        [requested("a-1"), failedEntry("a-1")],
        liveOwner,
      ),
    ).toEqual([]);
  });

  it("drops approvals requested before a later interrupt", () => {
    expect(
      pendingAgentApprovalIdsForTimeline(
        [requested("a-1"), interrupted()],
        liveOwner,
      ),
    ).toEqual([]);
  });

  it("keeps approvals requested after the last interrupt", () => {
    expect(
      pendingAgentApprovalIdsForTimeline(
        [interrupted(), requested("a-2")],
        liveOwner,
      ),
    ).toEqual(["a-2"]);
  });

  it("never blocks the composer when the owning turn is not live (D25 stale-question unblocking)", () => {
    // The owning assistant message reached a terminal status (stream error,
    // completed/stopped message, terminal task): a dead turn's unresolved
    // question must not keep the composer blocked.
    expect(
      pendingAgentApprovalIdsForTimeline([requested("a-1")], terminalOwner),
    ).toEqual([]);
  });

  it("does not block the composer for a ghost approval on an earlier terminal turn while a newer turn streams (#2318)", () => {
    // Turn A is terminal but its AskUserQuestion was never answered; Turn B is
    // streaming. The SESSION is streaming, yet the ghost approval's OWN turn (A)
    // is dead, so it must not block the composer. Gating on the session-global
    // stream instead was the #2318 defect.
    const entries = [requested("a-1", "assistant-A")];
    const messages = [
      assistantMsg("assistant-A", "complete"),
      assistantMsg("assistant-B", "streaming"),
    ];
    expect(pendingAgentApprovalIdsForTimeline(entries, messages)).toEqual([]);
  });

  it("a terminal-owner unresolved question with a future deadline and a later streaming turn is non-blocking", () => {
    // The owning turn has finalized (terminal) but the question is NOT settled —
    // its sticky deadline is still in the future — and a later turn streams. Per
    // #2318 the composer stays ENABLED; the answerable form reappears only after
    // the deadline sweep settles it (timed_out_answerable). The classifier does not
    // read deadlines, so "future deadline" here is simply an unsettled question.
    const entries = [
      entry("req-q-1", {
        providerEvent: "provided_agent.approval.requested",
        approvalId: "q-1",
        toolName: "AskUserQuestion",
        messageId: "assistant-A",
      }),
    ];
    const messages = [
      assistantMsg("assistant-A", "complete"),
      assistantMsg("assistant-B", "streaming"),
    ];
    expect(pendingAgentApprovalIdsForTimeline(entries, messages)).toEqual([]);
  });

  it("keeps a legacy approval without messageId blocking while a turn is live", () => {
    // A pre-deploy `approval.requested` entry carries no messageId, so it cannot be
    // judged per-owning-message; it falls back to session-global liveness — a
    // genuinely-live legacy question must keep blocking (CodeRabbit review).
    const legacy = entry("req-legacy", {
      providerEvent: "provided_agent.approval.requested",
      approvalId: "legacy-1",
    });
    expect(pendingAgentApprovalIdsForTimeline([legacy], liveOwner)).toEqual([
      "legacy-1",
    ]);
    // ...and dead once no turn is live (the session is terminal).
    expect(pendingAgentApprovalIdsForTimeline([legacy], terminalOwner)).toEqual(
      [],
    );
  });

  it("blocks the composer for an unresolved question on a live owning turn", () => {
    // Mirror of the ghost case: the approval's OWN owning turn (B) is live, so it
    // is actionable and composer-blocking even though an unrelated terminal turn
    // (A) also exists.
    const entries = [requested("a-1", "assistant-B")];
    const messages = [
      assistantMsg("assistant-A", "complete"),
      assistantMsg("assistant-B", "streaming"),
    ];
    expect(pendingAgentApprovalIdsForTimeline(entries, messages)).toEqual([
      "a-1",
    ]);
  });

  it("never blocks the composer for a timed-out-answerable question, even on a terminal owning turn (D25)", () => {
    const entries = [
      entry("req-q-1", {
        providerEvent: "provided_agent.approval.requested",
        approvalId: "q-1",
        toolName: "AskUserQuestion",
        messageId: OWNER,
      }),
      entry("settle-q-1", {
        providerEvent: "provided_agent.approval.resolved",
        approvalId: "q-1",
        resolutionSource: "idle_timeout",
        decision: "cancelled",
      }),
    ];
    // Settled → never blocks, regardless of owning-turn liveness; and the card is
    // still actionable (its sub-state stays timed_out_answerable).
    expect(pendingAgentApprovalIdsForTimeline(entries, terminalOwner)).toEqual(
      [],
    );
    expect(pendingAgentApprovalIdsForTimeline(entries, liveOwner)).toEqual([]);
    expect(classifyTimedOutQuestions(entries, []).get("q-1")).toBe(
      "timed_out_answerable",
    );
  });
});

describe("classifyAgentApprovals dead subset", () => {
  it("classifies an unresolved approval on a dead owning turn as dead, not answered", () => {
    const classification = classifyAgentApprovals(
      [requested("a-1")],
      terminalOwner,
    );
    expect(classification.dead.has("a-1")).toBe(true);
    expect(classification.settledOrDead.has("a-1")).toBe(true);
  });

  it("classifies a ghost approval on an earlier terminal turn as dead while a newer turn streams (#2318)", () => {
    const classification = classifyAgentApprovals(
      [requested("a-1", "assistant-A")],
      [
        assistantMsg("assistant-A", "complete"),
        assistantMsg("assistant-B", "streaming"),
      ],
    );
    expect(classification.dead.has("a-1")).toBe(true);
    expect(classification.pending).toEqual([]);
  });

  it("classifies an approval requested before a later interrupt as dead", () => {
    const classification = classifyAgentApprovals(
      [requested("a-1"), interrupted()],
      liveOwner,
    );
    expect(classification.dead.has("a-1")).toBe(true);
  });

  it("keeps a genuinely resolved approval out of the dead set", () => {
    const classification = classifyAgentApprovals(
      [requested("a-1"), resolvedEntry("a-1")],
      terminalOwner,
    );
    expect(classification.dead.has("a-1")).toBe(false);
    expect(classification.settledOrDead.has("a-1")).toBe(true);
  });

  it("classifies a provider NACK as failed and dead without resolving it", () => {
    const classification = classifyAgentApprovals(
      [requested("a-1"), failedEntry("a-1")],
      liveOwner,
    );
    expect(classification.failed.has("a-1")).toBe(true);
    expect(classification.dead.has("a-1")).toBe(true);
    expect(classification.settledOrDead.has("a-1")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// classifyTimedOutQuestions (chat_panel_runtime D25, plan
// late_answerable_question v1): timed-out AskUserQuestion sub-states derived
// from durable timeline evidence only — idle_timeout settlement ± the
// late_answered marker ± supersession evidence (a user message or a newer
// question request after the settlement). Order-independent by construction
// so reload-order permutations classify identically.
// ---------------------------------------------------------------------------
describe("classifyTimedOutQuestions (D25 timed-out question sub-states)", () => {
  function positioned(
    id: string,
    metadata: Record<string, unknown>,
    createdAt: string,
    sequence: number,
  ): TimelineEntry {
    return { ...entry(id, metadata), createdAt, sequence };
  }

  const questionRequest = (
    approvalId: string,
    createdAt = "2026-05-10T00:00:01Z",
    sequence = 1,
  ) =>
    positioned(
      `req-${approvalId}`,
      {
        providerEvent: "provided_agent.approval.requested",
        approvalId,
        toolName: "AskUserQuestion",
      },
      createdAt,
      sequence,
    );
  const settlement = (
    approvalId: string,
    createdAt = "2026-05-10T00:00:02Z",
    sequence = 2,
  ) =>
    positioned(
      `settle-${approvalId}`,
      {
        providerEvent: "provided_agent.approval.resolved",
        approvalId,
        resolutionSource: "idle_timeout",
        decision: "cancelled",
      },
      createdAt,
      sequence,
    );
  const marker = (approvalId: string) =>
    positioned(
      `marker-${approvalId}`,
      {
        providerEvent: "provided_agent.approval.late_answered",
        approvalId,
        lateAnswerMessageId: "late-answer-message-1",
      },
      "2026-05-10T00:00:06Z",
      4,
    );
  const userMessage = (createdAt: string) => ({ role: "user", createdAt });

  it("classifies a settlement-only timeout as timed_out_answerable", () => {
    const subStates = classifyTimedOutQuestions(
      [questionRequest("q-1"), settlement("q-1")],
      [],
    );
    expect(subStates.get("q-1")).toBe("timed_out_answerable");
  });

  it("classifies a marker-bearing timeout as late_answered", () => {
    const subStates = classifyTimedOutQuestions(
      [questionRequest("q-1"), settlement("q-1"), marker("q-1")],
      [],
    );
    expect(subStates.get("q-1")).toBe("late_answered");
  });

  it("lets the marker win over supersession evidence (the late answer IS a later user message)", () => {
    const subStates = classifyTimedOutQuestions(
      [questionRequest("q-1"), settlement("q-1"), marker("q-1")],
      [userMessage("2026-05-10T00:00:05Z")],
    );
    expect(subStates.get("q-1")).toBe("late_answered");
  });

  it("classifies a later user message as supersession", () => {
    const subStates = classifyTimedOutQuestions(
      [questionRequest("q-1"), settlement("q-1")],
      [userMessage("2026-05-10T00:05:00Z")],
    );
    expect(subStates.get("q-1")).toBe("timed_out_superseded");
  });

  it("does not treat a user message at or before the settlement instant as supersession", () => {
    const subStates = classifyTimedOutQuestions(
      [questionRequest("q-1"), settlement("q-1")],
      [
        userMessage("2026-05-10T00:00:00Z"),
        userMessage("2026-05-10T00:00:02Z"),
      ],
    );
    expect(subStates.get("q-1")).toBe("timed_out_answerable");
  });

  it("ignores later assistant messages for supersession (user messages only)", () => {
    const subStates = classifyTimedOutQuestions(
      [questionRequest("q-1"), settlement("q-1")],
      [{ role: "assistant", createdAt: "2026-05-10T00:05:00Z" }],
    );
    expect(subStates.get("q-1")).toBe("timed_out_answerable");
  });

  it("classifies a newer question request as supersession", () => {
    const subStates = classifyTimedOutQuestions(
      [
        questionRequest("q-1"),
        settlement("q-1"),
        questionRequest("q-2", "2026-05-10T00:00:10Z", 5),
      ],
      [],
    );
    expect(subStates.get("q-1")).toBe("timed_out_superseded");
  });

  it("does not treat an earlier question request as supersession", () => {
    const subStates = classifyTimedOutQuestions(
      [
        questionRequest("q-0", "2026-05-10T00:00:00Z", 0),
        questionRequest("q-1"),
        settlement("q-1"),
      ],
      [],
    );
    expect(subStates.get("q-1")).toBe("timed_out_answerable");
  });

  it("does not classify non-question approvals or ordinary resolutions", () => {
    const subStates = classifyTimedOutQuestions(
      [
        // Non-question tool timed out: not late-answerable (backend gate item 1).
        positioned(
          "req-bash",
          {
            providerEvent: "provided_agent.approval.requested",
            approvalId: "bash-1",
            toolName: "Bash",
          },
          "2026-05-10T00:00:01Z",
          1,
        ),
        settlement("bash-1"),
        // Ordinary in-time resolution: no idle_timeout resolutionSource.
        questionRequest("q-2"),
        positioned(
          "resolve-q-2",
          {
            providerEvent: "provided_agent.approval.resolved",
            approvalId: "q-2",
            decision: "approved",
          },
          "2026-05-10T00:00:03Z",
          3,
        ),
      ],
      [],
    );
    expect(subStates.size).toBe(0);
  });

  it("does not classify a settlement whose question request is outside the loaded window", () => {
    const subStates = classifyTimedOutQuestions([settlement("q-1")], []);
    expect(subStates.size).toBe(0);
  });

  it("classifies identically under reload-order permutations", () => {
    const entries = [
      questionRequest("q-1"),
      settlement("q-1"),
      marker("q-1"),
      questionRequest("q-2", "2026-05-10T00:00:10Z", 5),
      settlement("q-2", "2026-05-10T00:00:12Z", 6),
    ];
    const reversed = [...entries].reverse();
    const forward = classifyTimedOutQuestions(entries, []);
    const backward = classifyTimedOutQuestions(reversed, []);
    // q-1 answered late; q-2's settlement has no later question/user message.
    expect(forward.get("q-1")).toBe("late_answered");
    expect(forward.get("q-2")).toBe("timed_out_answerable");
    expect(backward.get("q-1")).toBe(forward.get("q-1"));
    expect(backward.get("q-2")).toBe(forward.get("q-2"));
  });
});
