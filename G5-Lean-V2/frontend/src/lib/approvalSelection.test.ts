import { describe, it, expect } from "vitest";
import {
  bulkSelection,
  hasOutstandingNeeds,
  outstandingPicks,
  pickKey,
  pickSelection,
  requestCandidates,
  requestNeedsOf,
} from "./approvalSelection";
import {
  approvalPathText,
  buildRequirementRoster,
  requestAgainTarget,
} from "./approvalRequirementRoster";
import { approvalVersionState } from "./approvalVersions";
import type {
  ApprovalDecisionLogEntryDTO,
  ApprovalRecipientNeedsDTO,
  ApprovalRequestDTO,
  OrgApprovalDefinitionDTO,
  WorkspaceOutgoingRequirementDTO,
} from "./types";

const human = (id: string) => ({
  userId: id,
  displayName: id,
  stillMember: true,
});
const none: ApprovalRecipientNeedsDTO = {
  agent: false,
  humans: [],
  randomHumanFrom: [],
};
function requirement(
  overrides: Partial<OrgApprovalDefinitionDTO> = {},
  outstanding: Partial<ApprovalRecipientNeedsDTO> = {},
): WorkspaceOutgoingRequirementDTO {
  return {
    definition: {
      id: "mixed",
      name: "Mixed",
      latestVersion: 1,
      agentType: "simple",
      humans: [human("owner"), human("ben"), human("rahul")],
      requireAllReviewers: false,
      ...overrides,
    } as OrgApprovalDefinitionDTO,
    callerCanDecide: false,
    state: "pending",
    approvers: [],
    decisionLog: [],
    requests: [],
    requestNeeds: {
      outstanding: { ...none, ...outstanding },
      repeat: { ...none, agent: true },
      agentRequestable: true,
      agentApproved: false,
    },
  };
}
const wait = (kind: "human" | "agent", id = "ben", reviewState?: string) =>
  ({
    requestId: `${kind}-${id}`,
    state: "pending",
    definitionVersion: 1,
    actorType: kind,
    actorUserId: kind === "human" ? id : null,
    requestedAt: "2026-10-30T10:00:00Z",
    agentReviews: reviewState ? [{ state: reviewState }] : [],
  }) as unknown as ApprovalRequestDTO;
const first = () => "rahul";
const labels = (
  r: WorkspaceOutgoingRequirementDTO[],
  mode: "all" | "agents",
  include = false,
  allowed = true,
) =>
  bulkSelection(r, "owner", mode, include, allowed, first).pairs.map(
    (p) => `${p.requirement.definition.name} → ${p.recipient.label}`,
  );

describe("approval request selection", () => {
  it("offers the agent then eligible humans, excluding self", () => {
    expect(requestCandidates(requirement(), "owner").map((c) => c.id)).toEqual([
      "agent",
      "ben",
      "rahul",
    ]);
  });
  it("excludes deactivated and departed members", () => {
    const r = requirement();
    r.definition.humans[1].deactivatedAt = "today";
    r.definition.humans[2].stillMember = false;
    expect(requestCandidates(r, "owner").map((c) => c.id)).toEqual(["agent"]);
  });
  it("marks active slots but lets a failed agent run be requested again", () => {
    const r = requirement();
    r.requests = [wait("human"), wait("agent", "", "failed")];
    const candidates = requestCandidates(r, "owner");
    expect(candidates.find((c) => c.id === "ben")!.active).toBe(true);
    expect(candidates.find((c) => c.id === "agent")!.active).toBe(false);
  });
  it("Request all sends the server's outstanding needs with one random human", () => {
    const r = requirement(
      {},
      { agent: true, humans: ["ben"], randomHumanFrom: ["ben", "rahul"] },
    );
    expect(labels([r], "all")).toEqual([
      "Mixed → Mixed agent",
      "Mixed → ben",
      "Mixed → rahul",
    ]);
  });
  it("never duplicates an active request even if the needs name it", () => {
    const r = requirement({}, { humans: ["ben"] });
    r.requests = [wait("human")];
    expect(labels([r], "all")).toEqual([]);
  });
  it.each(["all", "agents"] as const)(
    "%s skips approved requirements unless included, then uses repeat needs",
    (mode) => {
      const r = requirement({}, { agent: true });
      r.state = "approved";
      r.requestNeeds!.agentApproved = true;
      expect(labels([r], mode)).toEqual([]);
      expect(labels([r], mode, true)).toEqual(["Mixed → Mixed agent"]);
    },
  );
  it("Request agents only follows agentRequestable and skips agent-approved requirements", () => {
    const humanOnly = requirement({ agentType: null, name: "Humans", id: "h" });
    const vetoed = requirement({ name: "Vetoed", id: "v" });
    vetoed.requestNeeds!.agentApproved = true;
    const busy = requirement({ name: "Busy", id: "b" });
    busy.requestNeeds!.agentRequestable = false;
    const fresh = requirement({ name: "Fresh", id: "f" });
    const selection = bulkSelection(
      [humanOnly, vetoed, busy, fresh],
      "owner",
      "agents",
      false,
    );
    expect(selection.pairs.map((p) => p.requirement.definition.name)).toEqual([
      "Fresh",
    ]);
    expect(
      Object.fromEntries(
        selection.skipped.map((s) => [s.requirement.definition.name, s.reason]),
      ),
    ).toEqual({
      Busy: "no new reviewer needed or an active request is pending",
      Humans: "no agent configured",
      Vetoed: "no new reviewer needed or an active request is pending",
    });
  });
  it("selects nothing while the request PR rule blocks requests", () => {
    const r = requirement({}, { agent: true });
    expect(labels([r], "all", false, false)).toEqual([]);
    expect(
      bulkSelection([r], "owner", "all", false, false).skipped[0].reason,
    ).toMatch(/open PR/);
  });
  it("Pick follows only explicit, non-active checkboxes", () => {
    const r = requirement();
    r.requests = [wait("human")];
    const picked = new Set([
      pickKey("mixed", "ben"),
      pickKey("mixed", "rahul"),
    ]);
    expect(
      pickSelection([r], "owner", picked).pairs.map((p) => p.recipient.id),
    ).toEqual(["rahul"]);
  });
  it("outstanding picks resolve the random human and are absent when approved", () => {
    const r = requirement({}, { randomHumanFrom: ["ben", "rahul"] });
    expect(outstandingPicks(r, "owner", first)).toEqual([
      { definitionId: "mixed", recipientId: "rahul" },
    ]);
    expect(hasOutstandingNeeds(r, "owner")).toBe(true);
    r.state = "approved";
    expect(hasOutstandingNeeds(r, "owner")).toBe(false);
    const idle = requirement();
    expect(hasOutstandingNeeds(idle, "owner")).toBe(false);
  });
  it("falls back to unaddressed slots when an older server omits request needs", () => {
    const r = requirement();
    delete r.requestNeeds;
    expect(requestNeedsOf(r, "owner").outstanding).toEqual({
      agent: true,
      humans: [],
      randomHumanFrom: [],
    });
    r.requests = [wait("agent")];
    expect(requestNeedsOf(r, "owner").outstanding.agent).toBe(false);
  });
});

describe("compact roster", () => {
  const decision = (
    over: Partial<ApprovalDecisionLogEntryDTO>,
  ): ApprovalDecisionLogEntryDTO => ({
    outcome: "approved",
    comment: null,
    definitionVersion: 1,
    superseded: false,
    reviewedGitHeads: null,
    createdAt: "2026-10-29T10:00:00Z",
    ...over,
  });
  it("orders the agent first then humans, and keeps an earlier settled response under a newer waiting request", () => {
    const r = requirement();
    r.requests = [
      {
        ...wait("human", "rahul"),
        state: "settled",
        requestId: "r1",
        actor: { userId: "rahul", displayName: "rahul" },
      },
      {
        ...wait("agent"),
        state: "settled",
        requestId: "a1",
        requestedAt: "2026-10-28T10:00:00Z",
      },
      {
        ...wait("agent"),
        requestId: "a2",
        requestedAt: "2026-10-30T10:00:00Z",
      },
      {
        ...wait("agent"),
        state: "cancelled",
        requestId: "a0",
        requestedAt: "2026-10-27T10:00:00Z",
      },
    ];
    r.decisionLog = [
      decision({ requestId: "a1", actorType: "agent", outcome: "rejected" }),
      decision({
        requestId: "r1",
        actorType: "human",
        actor: { userId: "rahul", displayName: "rahul" },
        outcome: "deferred",
      }),
    ];
    const rows = buildRequirementRoster(r);
    expect(rows.map((row) => row.name)).toEqual(["Mixed agent", "rahul"]);
    expect(rows[0].primary).toMatchObject({
      type: "request",
      request: { requestId: "a2" },
    });
    expect(rows[0].prior?.requestId).toBe("a1");
    expect(requestAgainTarget(rows[0])).toBeNull();
    expect(requestAgainTarget(rows[1])).toBe("primary");
  });
  it("offers Request again beside an earlier rejection only after a cancelled request, and never for Approved", () => {
    const r = requirement();
    r.requests = [
      {
        ...wait("agent"),
        state: "settled",
        requestId: "a1",
        requestedAt: "2026-10-28T10:00:00Z",
      },
      { ...wait("agent"), state: "cancelled", requestId: "a2" },
    ];
    r.decisionLog = [
      decision({ requestId: "a1", actorType: "agent", outcome: "rejected" }),
    ];
    expect(requestAgainTarget(buildRequirementRoster(r)[0])).toBe("prior");
    r.decisionLog[0].outcome = "approved";
    expect(requestAgainTarget(buildRequirementRoster(r)[0])).toBeNull();
    r.decisionLog = [];
    expect(requestAgainTarget(buildRequirementRoster(r)[0])).toBeNull();
  });
  it("words an all-of path that needs everyone", () => {
    expect(
      approvalPathText({
        kind: "needed",
        approvers: [
          {
            actorType: "agent",
            userId: null,
            displayName: "Security review agent",
          },
          { actorType: "human", userId: "t", displayName: "Tim" },
        ],
        requireAll: true,
        allHumansRequired: true,
      }),
    ).toBe(
      "Approval from Security review agent and Tim will meet this requirement. All configured humans have to approve to meet this requirement.",
    );
  });
});

describe("pushed approval vector currentness", () => {
  const a = { repoId: "code", headCommit: "a".repeat(40) },
    b = { repoId: "docs", headCommit: "b".repeat(40) };
  it("compares complete vectors independently of order or outcome", () =>
    expect(approvalVersionState([a, b], [b, a])).toBe("matching"));
  it("handles changed commits, added and removed members", () => {
    for (const current of [
      [a],
      [a, b, { repoId: "new", headCommit: "c".repeat(40) }],
      [a, { ...b, headCommit: "c".repeat(40) }],
    ])
      expect(approvalVersionState([a, b], current)).toBe("changed");
  });
  it.each([null, [], {}, [{ repoId: "code", headCommit: "" }], [a, a]])(
    "keeps missing/malformed evidence unknown: %j",
    (value) => expect(approvalVersionState(value, [a, b])).toBe("unknown"),
  );
});
