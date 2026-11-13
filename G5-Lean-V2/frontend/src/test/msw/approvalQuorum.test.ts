import { afterEach, expect, it } from "vitest";
import { api } from "../../lib/api";
import type {
  WorkspaceApprovalsStateDTO,
  OrgApprovalDefinitionDTO,
} from "../../lib/types";
import {
  resetMockWorkspaceApprovals,
  seedMockWorkspaceApprovals,
} from "./handlers";

afterEach(resetMockWorkspaceApprovals);
const person = (userId: string) => ({
  userId,
  displayName: userId,
  email: `${userId}@test.example`,
  avatarUrl: null,
  stillMember: true,
});
const definition: OrgApprovalDefinitionDTO = {
  id: "quorum",
  latestVersion: 1,
  name: "Quorum",
  description: null,
  humans: [person("user-alex"), person("user-other")],
  agentType: null,
  agentPrompt: null,
  invalidatesPrevious: false,
  phases: ["ie"],
  createdBy: person("creator"),
  firstCreatedBy: person("creator"),
  createdAt: "2026-10-13T00:00:00Z",
  firstCreatedAt: "2026-10-13T00:00:00Z",
};
it.each([
  {
    name: "one human approval",
    all: false,
    other: "pending",
    expected: "approved",
  },
  {
    name: "a denial vetoes one approval",
    all: false,
    other: "rejected",
    expected: "rejected",
  },
  {
    name: "all human approvals",
    all: true,
    other: "pending",
    expected: "pending",
  },
  {
    name: "legacy all-human policy",
    all: undefined,
    other: "pending",
    expected: "pending",
  },
  {
    name: "both humans approve",
    all: true,
    other: "approved",
    expected: "approved",
  },
  {
    name: "Any-of accepts a human while the agent remains pending",
    all: false,
    other: "pending",
    agent: "pending",
    expected: "approved",
  },
  {
    name: "required agent approves",
    all: false,
    other: "pending",
    agent: "approved",
    expected: "approved",
  },
  {
    name: "Any-of human approval overrides an agent rejection",
    all: false,
    other: "pending",
    agent: "rejected",
    expected: "approved",
  },
  {
    name: "Any-of agent rejection blocks when override is disabled",
    all: false,
    override: false,
    other: "pending",
    agent: "rejected",
    expected: "rejected",
  },
  {
    name: "All still requires the agent despite human override",
    all: true,
    other: "approved",
    agent: "rejected",
    expected: "rejected",
  },
  {
    name: "a human rejection blocks an approving agent",
    all: false,
    other: "rejected",
    agent: "approved",
    expected: "rejected",
  },
  {
    name: "configured humans have no eligible rows",
    all: false,
    other: "pending",
    empty: true,
    expected: "pending",
  },
] as const)("MSW projects $name after a decision", async (scenario) => {
  const agent = "agent" in scenario ? scenario.agent : undefined;
  const state: WorkspaceApprovalsStateDTO = {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "target",
      targetWorkspaceName: "Target",
      history: [],
      requirements: [
        {
          definition: {
            ...definition,
            requireAllReviewers: scenario.all,
            humanApprovalsOverrideAgent:
              "override" in scenario ? scenario.override : true,
            agentType: agent ? "simple" : null,
          },
          phase: "ie",
          state: "pending",
          callerCanDecide: true,
          approvers:
            "empty" in scenario
              ? []
              : [
                  {
                    ...person("user-alex"),
                    state: "pending",
                    comment: null,
                    decidedAt: null,
                  },
                  {
                    ...person("user-other"),
                    state: scenario.other,
                    comment: null,
                    decidedAt: null,
                  },
                ],
          decisionLog:
            agent && agent !== "pending"
              ? [
                  {
                    actorType: "agent",
                    outcome: agent,
                    comment: null,
                    definitionVersion: 1,
                    superseded: false,
                    reviewedGitHeads: [],
                    createdAt: "2026-10-13T00:00:00Z",
                  },
                ]
              : [],
        },
      ],
    },
  };
  seedMockWorkspaceApprovals("ws-quorum", state);
  const result = await api.post<WorkspaceApprovalsStateDTO>(
    "/api/v1/workspaces/ws-quorum/approvals/decisions",
    { definitionId: "quorum", outcome: "approved", phase: "ie" },
  );
  expect(result.outgoing?.requirements[0].state).toBe(scenario.expected);
  expect(result.outgoing?.mergeAdmission?.admitted).toBe(
    scenario.expected === "approved",
  );
});
