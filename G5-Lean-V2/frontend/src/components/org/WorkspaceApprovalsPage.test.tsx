import type { ReactElement } from "react";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, delay, http } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import {
  resetMockOrgApprovalDefinitions,
  resetMockWorkspaceApprovals,
  seedMockOrgApprovalDefinition,
  seedMockWorkspaceApprovals as seedApprovalsState,
} from "../../test/msw/handlers";
import {
  createMockUser,
  MOCK_ADMIN_SCOPES,
  mockWorkspaceDetails,
} from "../../test/fixtures";
import type {
  ApprovalRequestDTO,
  OrgApprovalDefinitionDTO,
  ReviewOverview,
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingRequirementDTO,
} from "../../lib/types";
import { WorkspaceApprovalsPageContent } from "./WorkspaceApprovalsPageContent";

const ORG_SLUG = "g5labs";
const ORG_ID = "org-g5labs";
const WS_ID = "ws-fork-1";
const ROUTE = `/${ORG_SLUG}/payments-refactor`;

function makeDefinition(
  overrides: Partial<OrgApprovalDefinitionDTO> = {},
): OrgApprovalDefinitionDTO {
  return {
    id: "apprdef-1",
    latestVersion: 1,
    name: "Production merge sign-off",
    description: "Required for merges into the production base workspace.",
    humans: [
      {
        // renderApp's mock user id — the caller must be a listed approver.
        userId: "user-alex",
        displayName: "Alex Chen",
        email: "alex@g5labs.dev",
        avatarUrl: null,
        stillMember: true,
      },
      {
        userId: "user-jordan",
        displayName: "Jordan Rivera",
        email: "jordan@g5labs.dev",
        avatarUrl: null,
        stillMember: true,
      },
    ],
    agentType: null,
    agentPrompt: null,
    invalidatesPrevious: false,
    createdBy: { userId: "user-alex", displayName: "Alex Chen" },
    createdAt: "2026-08-11T09:00:00.000Z",
    firstCreatedBy: { userId: "user-alex", displayName: "Alex Chen" },
    firstCreatedAt: "2026-07-28T09:00:00.000Z",
    ...overrides,
  };
}

function forkState(): WorkspaceApprovalsStateDTO {
  const definition = makeDefinition();
  return {
    requiredInto: [
      {
        definition: makeDefinition({
          id: "apprdef-into",
          name: "Fork-inbound review",
        }),
        attachedBy: { userId: "user-alex", displayName: "Alex Chen" },
        attachedAt: "2026-08-11T10:00:00.000Z",
      },
    ],
    attachmentHistory: [
      {
        definitionId: "apprdef-into",
        definitionName: "Fork-inbound review",
        retired: false,
        actor: { userId: "user-alex", displayName: "Alex Chen" },
        createdAt: "2026-08-11T10:00:00.000Z",
      },
    ],
    outgoing: {
      targetWorkspaceId: "ws-base",
      targetWorkspaceName: "Base",
      requirements: [
        {
          definition,
          state: "pending",
          approvers: [
            {
              userId: "user-alex",
              displayName: "Alex Chen",
              email: "alex@g5labs.dev",
              stillMember: true,
              state: "pending",
              comment: null,
              decidedAt: null,
              requestId: "11111111-1111-4111-8111-111111111111",
            },
            {
              userId: "user-jordan",
              displayName: "Jordan Rivera",
              email: "jordan@g5labs.dev",
              stillMember: true,
              state: "rejected",
              comment: "Refund flows are missing.",
              decidedAt: "2026-08-11T11:00:00.000Z",
              requestId: "11111111-2222-4111-8111-111111111111",
            },
          ],
          callerCanDecide: true,
          requests: [
            {
              requestId: "11111111-1111-4111-8111-111111111111",
              actorType: "human",
              actorUserId: "user-alex",
              actor: { userId: "user-alex", displayName: "Alex Chen" },
              requestedBy: {
                userId: "user-jordan",
                displayName: "Jordan Rivera",
              },
              state: "pending",
              definitionVersion: 1,
              definitionName: definition.name,
              requestedAt: "2026-08-11T09:30:00.000Z",
              closedAt: null,
              agentReviews: [],
            },
            {
              requestId: "11111111-2222-4111-8111-111111111111",
              actorType: "human",
              actorUserId: "user-jordan",
              actor: { userId: "user-jordan", displayName: "Jordan Rivera" },
              requestedBy: { userId: "user-alex", displayName: "Alex Chen" },
              state: "settled",
              definitionVersion: 1,
              definitionName: definition.name,
              requestedAt: "2026-08-11T09:40:00.000Z",
              closedAt: "2026-08-11T11:00:00.000Z",
              agentReviews: [],
            },
          ],
          decisionLog: [
            {
              requestId: "11111111-2222-4111-8111-111111111111",
              outcome: "rejected",
              comment: "Refund flows are missing.",
              actor: { userId: "user-jordan", displayName: "Jordan Rivera" },
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: [],
              createdAt: "2026-08-11T11:00:00.000Z",
            },
          ],
        },
      ],
    },
  };
}

/** forkState with the caller already approved (withdrawal entry point). */
/** Exposes the router's current search string (picker library-link test). */
function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location-probe">{location.search}</div>;
}

function renderTab(opts?: {
  role?: "admin" | "member" | "viewer";
  scopes?: string[];
  /** Query string appended to the route (`?view=approvals&phase=be`). */
  search?: string;
  onOpenEvidenceDoc?: (docId: string, path: string) => void;
  canResolveEvidenceDoc?: (docId: string, path: string) => boolean;
}) {
  const ui: ReactElement = (
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <>
            <WorkspaceApprovalsPageContent workspaceId={WS_ID} />
            <LocationProbe />
          </>
        }
      />
    </Routes>
  );
  return renderApp(ui, {
    route: `${ROUTE}${opts?.search ?? ""}`,
    user: createMockUser({
      orgs: [
        {
          orgId: ORG_ID,
          orgName: "G5 Labs",
          orgSlug: ORG_SLUG,
          role: opts?.role ?? "admin",
        },
      ],
      scopes: opts?.scopes ?? MOCK_ADMIN_SCOPES,
    }),
  });
}

// Human actions use the shared, PR-scoped production dialog. Legacy merge-gate
// rows stay readable; they cannot start another decision protocol.
function seedFormalReviews(owner = false) {
  const definition = makeDefinition();
  seedMockOrgApprovalDefinition(ORG_ID, definition);
  const heads = [
    {
      repoId: "code",
      workspaceId: WS_ID,
      branchName: "fork",
      headCommit: "a".repeat(40),
    },
    {
      repoId: "docs",
      workspaceId: WS_ID,
      branchName: "fork",
      headCommit: "b".repeat(40),
    },
  ];
  const data: ReviewOverview = {
    workspace: {
      ownerUserId: owner ? "user-alex" : "user-jordan",
      targetWorkspaceId: "ws-base",
      phase: "ie",
      readOnly: false,
    },
    pr: {
      id: "pr-one",
      title: "Implementation",
      description: "Decisions",
      sourceHeads: heads,
      createdAt: "2026-10-12T10:00:00Z",
    },
    heads,
    baseHeads: heads,
    parentHeads: heads,
    pushedAt: {},
    repos: [],
    requests: [
      {
        id: "review-task",
        definitionId: definition.id,
        definitionVersion: 1,
        name: definition.name,
        phase: "ie",
        actorUserId: owner ? "user-jordan" : "user-alex",
        requestedBy: owner ? "user-alex" : "user-jordan",
        state: "pending",
        createdAt: "2026-10-12T11:00:00Z",
        closedAt: null,
      },
    ],
    decisions: [],
  };
  server.use(
    http.get(`*/api/v1/workspaces/${WS_ID}/review-flow`, () =>
      HttpResponse.json({ data }),
    ),
  );
  activeFlow = data;
  return data;
}

let activeFlow: ReviewOverview;
function seedMockWorkspaceApprovals(
  id: string,
  state: WorkspaceApprovalsStateDTO,
) {
  seedApprovalsState(id, state);
  if (!activeFlow) return;
  activeFlow.requests = (state.outgoing?.requirements ?? []).flatMap((r) =>
    (r.requests ?? [])
      .filter((request) => request.actorType === "human")
      .map((request) => ({
        id: request.requestId,
        definitionId: r.definition.id,
        definitionVersion: request.definitionVersion,
        name: request.definitionName,
        phase: request.phase ?? "ie",
        actorUserId: request.actorUserId!,
        requestedBy: request.requestedBy?.userId ?? "",
        state: request.state,
        createdAt: request.requestedAt,
        closedAt: request.closedAt,
        note: request.note,
      })),
  );
  activeFlow.decisions = (state.outgoing?.requirements ?? []).flatMap((r) =>
    r.decisionLog
      .filter((d) => d.actorType !== "agent")
      .map((d, i) => ({
        id: `decision-${i}`,
        requestId: d.requestId!,
        definitionId: r.definition.id,
        name: r.definition.name,
        phase: "ie" as const,
        actorUserId: d.actor?.userId ?? "",
        outcome: d.outcome,
        comment: d.comment,
        reviewedGitHeads: activeFlow.heads,
        createdAt: d.createdAt,
      })),
  );
}

async function openApprovalTimeline(name = "Production merge sign-off") {
  await screen.findByTestId("workspace-approvals-outgoing");
  const legacy = screen.queryByTestId("workspace-approvals-legacy");
  if (
    legacy &&
    !legacy.hasAttribute("open") &&
    within(legacy).queryByRole("button", {
      name: `Timeline for ${name}`,
      hidden: true,
    })
  )
    await userEvent.click(
      within(legacy).getByTestId("workspace-approvals-legacy-toggle"),
    );
  const button = await screen.findByRole("button", {
    name: `Timeline for ${name}`,
  });
  if (button.getAttribute("aria-expanded") === "false")
    await userEvent.click(button);
  return screen.findByRole("region", { name: `Timeline for ${name}` });
}

describe("WorkspaceApprovalsPageContent", () => {
  beforeEach(() => {
    sessionStorage.clear();
    resetMockWorkspaceApprovals();
    resetMockOrgApprovalDefinitions();
    // Existing decision-state fixtures describe a submitted workspace PR.
    seedFormalReviews().requests = [];
    server.use(
      http.get(`/api/v1/workspaces/${WS_ID}`, () =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails.base,
            id: WS_ID,
            name: "payments-refactor",
            ownerUserId: "user-alex",
            isBase: false,
            readOnly: false,
          },
        }),
      ),
    );
  });

  it("keeps three sections and visible configuration without inventing a parent requirement list", async () => {
    seedMockWorkspaceApprovals(WS_ID, { ...forkState(), outgoing: null });
    renderTab();
    await screen.findByTestId("approvals-reviews-toggle");
    expect(
      screen.queryByTestId("workspace-approvals-outgoing"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("This workspace has no parent merge target."),
    ).toBeVisible();
    expect(
      screen
        .getAllByRole("heading", { level: 2 })
        .map((heading) => heading.textContent),
    ).toEqual([
      "Approvals Required to merge into a parent workspace",
      expect.stringContaining("Reviews"),
      "Configure Approvals Required to merge into this workspace",
    ]);
    const requiredSection = screen.getByTestId(
      "workspace-approvals-required-into",
    );
    expect(
      within(requiredSection).getByTestId("workspace-approvals-require"),
    ).toBeVisible();
    expect(
      screen.queryByTestId("workspace-approvals-advanced-toggle"),
    ).toBeNull();
    expect(
      screen.queryByRole("heading", { name: "Approval history" }),
    ).toBeNull();
  });

  it("keeps approval history visible and blocks mutations when workspace details fail", async () => {
    const mutations = { count: 0 };
    server.use(
      http.get("/api/v1/workspaces/:workspaceId", () =>
        HttpResponse.json(
          { error: { message: "workspace details unavailable" } },
          { status: 500 },
        ),
      ),
      http.post("/api/v1/workspaces/:workspaceId/approvals/:action", () => {
        mutations.count += 1;
        return HttpResponse.json({ data: {} });
      }),
    );
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab();

    expect(
      await screen.findByTestId("workspace-approvals-outgoing"),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "Approval history" }),
    ).toBeNull();
    expect(await openApprovalTimeline()).toBeVisible();
    expect(
      screen.queryByTestId("workspace-approvals-error"),
    ).not.toBeInTheDocument();

    const approve = await screen.findByRole("button", {
      name: "Approve",
    });
    const reject = screen.getByRole("button", { name: "Reject" });
    expect(approve).toBeDisabled();
    expect(reject).toBeDisabled();
    const user = userEvent.setup();
    await user.click(approve);
    await user.click(reject);
    expect(mutations.count).toBe(0);
  });

  it("keeps the page, its review threads and a typed reply when a refresh fails", async () => {
    const unavailable = () =>
      HttpResponse.json(
        {
          error: {
            code: "SERVICE_UNAVAILABLE",
            message: "The service is restarting",
            requestId: "request-down",
          },
        },
        { status: 503 },
      );
    const detail = () =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails.base,
          id: WS_ID,
          name: "payments-refactor",
          ownerUserId: "user-alex",
          isBase: false,
          readOnly: false,
        },
      });
    const approvalsState = forkState();
    seedMockWorkspaceApprovals(WS_ID, approvalsState);
    const approvalsHandler = () => HttpResponse.json({ data: approvalsState });
    server.use(
      http.get(`*/api/v1/workspaces/${WS_ID}`, detail),
      http.get(`*/api/v1/workspaces/${WS_ID}/review/publications`, () =>
        HttpResponse.json({
          data: {
            publications: [
              {
                id: "pub-1",
                orgId: ORG_ID,
                sourceWorkspaceId: WS_ID,
                reviewWorkspaceId: "ws-review-1",
                reviewerUserId: "user-jordan",
                authorKind: "human",
                authorLabel: "jordan",
                round: 1,
                baseCommitVector: [],
                summary: "One question.",
                verdict: "comment_only",
                scores: null,
                supersededBy: null,
                createdAt: "2026-09-23T14:31:00.000Z",
                openCount: 1,
                acceptedCount: 0,
                rejectedCount: 0,
                resolvedCount: 0,
                supersededCount: 0,
                artifactCount: 0,
              },
            ],
            findings: [],
            reviewerDisplayNames: { "user-jordan": "Jordan Rivera" },
          },
        }),
      ),
      http.get(`*/api/v1/workspaces/${WS_ID}/review/suggestions`, () =>
        HttpResponse.json({
          data: {
            suggestions: [
              {
                id: "sug-1",
                publicationId: "pub-1",
                findingId: null,
                repoId: "repo-api",
                path: "README.md",
                kind: "comment",
                beforeLines: null,
                afterLines: null,
                anchorStart: 12,
                contentHash: null,
                anchorContext: null,
                authorKind: "human",
                authorLabel: "jordan",
                note: "Is this still accurate?",
                commentId: "c-1",
                status: "open",
                statusBy: null,
                statusAt: null,
                supersedes: null,
                adoptedFrom: null,
                createdAt: "2026-09-23T14:31:00.000Z",
              },
            ],
          },
        }),
      ),
      http.get(
        `*/api/v1/workspaces/${WS_ID}/review/suggestions/:id/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const view = renderTab();
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("approvals-reviews-toggle"));
    await user.click(
      await screen.findByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const reply = await screen.findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(reply, "Draft answer");

    // The approvals cannot be refreshed: the page stays as last loaded.
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/approvals", unavailable),
    );
    await view.queryClient.invalidateQueries({
      queryKey: ["workspace-approvals", WS_ID],
    });
    expect(
      await screen.findByTestId("workspace-approvals-refresh-error"),
    ).toHaveTextContent("Could not refresh approvals");
    expect(
      screen.queryByTestId("workspace-approvals-error"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("review-suggestion-reply-input-sug-1")).toBe(
      reply,
    );
    expect(reply).toHaveValue("Draft answer");
    // Approval actions wait for current approvals.
    expect(
      within(screen.getByTestId("workspace-approvals-outgoing")).getByRole(
        "button",
        { name: "Approve" },
      ),
    ).toBeDisabled();
    expect(
      within(screen.getByTestId("workspace-approvals-outgoing")).getByRole(
        "button",
        { name: "Reject" },
      ),
    ).toBeDisabled();

    // Nor can the workspace: its review threads stay, and approval actions
    // wait for current details as before.
    server.use(http.get(`*/api/v1/workspaces/${WS_ID}`, unavailable));
    await view.queryClient.invalidateQueries({ queryKey: ["workspace"] });
    await waitFor(() =>
      expect(view.queryClient.getQueryState(["workspace", WS_ID])?.status).toBe(
        "error",
      ),
    );
    expect(screen.getByTestId("workspace-approvals-reviews")).toBeVisible();
    expect(screen.getByTestId("review-suggestion-reply-input-sug-1")).toBe(
      reply,
    );
    expect(reply).toHaveValue("Draft answer");
    expect(
      within(screen.getByTestId("workspace-approvals-outgoing")).getByRole(
        "button",
        { name: "Approve" },
      ),
    ).toBeDisabled();
    expect(
      within(screen.getByTestId("workspace-approvals-outgoing")).getByRole(
        "button",
        { name: "Reject" },
      ),
    ).toBeDisabled();

    // Back: the statuses go; the reply is still there to send.
    server.use(
      http.get(`*/api/v1/workspaces/${WS_ID}`, detail),
      http.get("/api/v1/workspaces/:workspaceId/approvals", approvalsHandler),
    );
    await view.queryClient.invalidateQueries();
    await waitFor(() =>
      expect(
        screen.queryByTestId("workspace-approvals-refresh-error"),
      ).not.toBeInTheDocument(),
    );
    expect(reply).toHaveValue("Draft answer");
  });

  it("renders loaded approvals read-only while workspace details are loading", async () => {
    const mutations = { count: 0 };
    server.use(
      http.get("/api/v1/workspaces/:workspaceId", async () => {
        await delay("infinite");
        return HttpResponse.json({ data: mockWorkspaceDetails.base });
      }),
      http.post("/api/v1/workspaces/:workspaceId/approvals/:action", () => {
        mutations.count += 1;
        return HttpResponse.json({ data: {} });
      }),
    );
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab();

    expect(
      await screen.findByTestId("workspace-approvals-outgoing"),
    ).toBeVisible();
    expect(
      screen.queryByRole("heading", { name: "Approval history" }),
    ).toBeNull();
    expect(await openApprovalTimeline()).toBeVisible();
    expect(
      screen.queryByTestId("workspace-approvals-loading"),
    ).not.toBeInTheDocument();

    const user = userEvent.setup();
    for (const name of ["Approve", "Reject"]) {
      const button = within(
        screen.getByTestId("workspace-approvals-outgoing"),
      ).getByRole("button", { name });
      expect(button).toBeDisabled();
      await user.click(button);
    }
    expect(mutations.count).toBe(0);
  });

  it("shows no decision buttons when the caller cannot decide", async () => {
    const state = forkState();
    state.outgoing!.requirements[0].callerCanDecide = false;
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    await screen.findByTestId("workspace-approvals-outgoing");
    for (const name of ["Approve", "Reject"]) {
      expect(
        within(screen.getByTestId("workspace-approvals-outgoing")).queryByRole(
          "button",
          { name },
        ),
      ).not.toBeInTheDocument();
    }
  });

  it("hides attach/retire affordances from non-admin members", async () => {
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab({ role: "member", scopes: ["workspaces:read"] });
    await screen.findByTestId("workspace-approvals-outgoing");
    // The end-of-page section stays readable but write-free.
    await screen.findByTestId("workspace-approvals-required-into");
    expect(
      screen.queryByTestId("workspace-approvals-require"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-approvals-required-menu"),
    ).not.toBeInTheDocument();
  });

  it("attaches library definitions through the require button-row's picker", async () => {
    const user = userEvent.setup();
    seedMockWorkspaceApprovals(WS_ID, forkState());
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({ id: "apprdef-new", name: "Security-sensitive paths" }),
    );
    renderTab();

    await user.click(await screen.findByTestId("workspace-approvals-require"));
    const card = (
      await screen.findAllByTestId("workspace-approvals-pick-card")
    ).find((c) => within(c).queryByText("Security-sensitive paths"))!;
    await user.click(card);
    await user.click(screen.getByTestId("workspace-approvals-picker-submit"));

    // Back on the page with the new requirement attached.
    const requiredSection = await screen.findByTestId(
      "workspace-approvals-required-into",
    );
    await waitFor(() => {
      expect(
        within(requiredSection).getByText("Security-sensitive paths"),
      ).toBeInTheDocument();
    });
  });

  it("links the picker to the library for creating a new definition", async () => {
    const user = userEvent.setup();
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab();

    await user.click(await screen.findByTestId("workspace-approvals-require"));
    await user.click(
      await screen.findByTestId("workspace-approvals-picker-new-definition"),
    );
    // Navigates to the org definitions library, where creation lives.
    expect(screen.getByTestId("location-probe")).toHaveTextContent(
      "view=approval-definitions",
    );
  });

  it("retires a requirement without adding internal configuration chatter to approval history", async () => {
    const user = userEvent.setup();
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab();

    await user.click(
      await screen.findByTestId("workspace-approvals-required-menu"),
    );
    await user.click(screen.getByTestId("workspace-approvals-stop-requiring"));
    await user.click(screen.getByTestId("workspace-approvals-confirm-retire"));

    await waitFor(() => {
      expect(
        screen.queryByTestId("workspace-approvals-required-row"),
      ).not.toBeInTheDocument();
    });

    // Approval history remains request-centric; attach/retire bookkeeping is
    // not presented as an approval request card.
    const feed = await openApprovalTimeline();
    expect(
      within(feed).getAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(2);
    expect(feed).not.toHaveTextContent("stopped requiring");
  });

  it("renders one immutable card per request and keeps human decision comments", async () => {
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab();

    const feed = await openApprovalTimeline();
    const entries = within(feed).getAllByTestId(
      "workspace-approvals-history-card",
    );
    expect(entries).toHaveLength(2);
    // Reviewers are grouped alphabetically; preserve each immutable receipt.
    expect(entries[1]).toHaveTextContent("Rejected");
    expect(
      within(entries[1]).getByText("Refund flows are missing."),
    ).toBeInTheDocument();
    expect(feed).not.toHaveTextContent("Fork-inbound review");
  });

  // -------------------------------------------------------------------------
  // Phase-scoped gate rows (g5.workbench.gates)
  // -------------------------------------------------------------------------

  /**
   * A definition bound to BE and IE projects two entries — one per phase —
   * each with the caller pending and able to decide; only the IE entry
   * gates the merge, so the server refuses admission while it is pending.
   */
  function twoPhaseForkState(): WorkspaceApprovalsStateDTO {
    const definition = makeDefinition({
      id: "apprdef-both",
      name: "Two-phase sign-off",
      phases: ["be", "ie"],
    });
    const callerApprover = (requestId: string) => ({
      userId: "user-alex",
      displayName: "Alex Chen",
      email: "alex@g5labs.dev",
      stillMember: true,
      state: "pending" as const,
      comment: null,
      decidedAt: null,
      requestId,
    });
    return {
      requiredInto: [],
      attachmentHistory: [],
      outgoing: {
        targetWorkspaceId: "ws-base",
        targetWorkspaceName: "Base",
        requirements: [
          {
            definition,
            state: "pending",
            approvers: [callerApprover("22222222-2222-4222-8222-222222222222")],
            callerCanDecide: true,
            decisionLog: [],
          },
        ],
        mergeAdmission: {
          admitted: false,
          reason:
            "Approval required before merging: Two-phase sign-off (pending)",
          requirements: [
            {
              definitionId: "apprdef-both",
              definitionName: "Two-phase sign-off",
              definitionVersion: 1,
              state: "pending",
            },
          ],
        },
      },
    };
  }

  it("keeps the whole workspace requirement list visible on a legacy phase URL", async () => {
    const state = twoPhaseForkState();
    state.outgoing!.requirements = [state.outgoing!.requirements[0]];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab({ search: "?view=approvals&phase=se" });
    const gate = await screen.findByTestId("workspace-approvals-outgoing");
    expect(gate).toHaveTextContent("Two-phase sign-off");
    expect(
      screen.queryByRole("group", { name: "Filter approvals by phase" }),
    ).toBeNull();
    expect(screen.queryByTestId("workspace-approvals-empty")).toBeNull();
  });

  it("isolates each approval timeline and combines its approver and status filters", async () => {
    const state = forkState();
    const requirement = state.outgoing!.requirements[0];
    state.outgoing!.history = [
      {
        definitionId: requirement.definition.id,
        definitionName: requirement.definition.name,
        requests: requirement.requests ?? [],
        decisionLog: requirement.decisionLog,
      },
      {
        definitionId: "apprdef-other",
        definitionName: "Security review",
        requests: [
          {
            requestId: "security-request",
            actorType: "human",
            actorUserId: "user-alex",
            actor: { userId: "user-alex", displayName: "Alex Chen" },
            requestedBy: {
              userId: "user-jordan",
              displayName: "Jordan Rivera",
            },
            state: "pending",
            definitionVersion: 1,
            definitionName: "Security review",
            requestedAt: "2026-08-12T10:00:00.000Z",
            closedAt: null,
            agentReviews: [],
          },
        ],
        decisionLog: [],
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const user = userEvent.setup();
    const feed = await openApprovalTimeline();
    const other = await openApprovalTimeline("Security review");
    expect(
      within(feed).getAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(2);
    expect(feed).not.toHaveTextContent("Security review");
    expect(
      within(other).getAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(1);
    expect(other).not.toHaveTextContent(requirement.definition.name);
    expect(
      screen.queryByLabelText(
        "Filter approval history by approval requirement",
      ),
    ).toBeNull();

    const approverFilter = within(feed).getByLabelText(
      "Filter approval history by approver",
    );
    await user.selectOptions(approverFilter, "user:user-jordan");
    expect(
      within(feed).getAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(1);
    expect(feed).toHaveTextContent("Rejected");
    expect(
      within(feed).getByText("Showing 1 of 2 timeline entries"),
    ).toBeVisible();
    const statusFilter = within(feed).getByLabelText(
      "Filter approval history by request status",
    );
    await user.selectOptions(statusFilter, "Active");
    expect(
      within(feed).queryAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(0);
    expect(
      within(feed).getByText("No history matches these filters."),
    ).toBeVisible();
    expect(
      within(other).getAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(1);
    await user.click(
      within(feed).getByRole("button", { name: "Clear filters" }),
    );
    expect(
      within(feed).getAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(2);
    expect(approverFilter).toHaveValue("all");
    expect(statusFilter).toHaveValue("all");
    expect(
      within(feed).getByText("Showing all 2 timeline entries"),
    ).toBeVisible();
  });

  it("groups each approval's timeline by reviewer with newest requests first", async () => {
    const state = forkState();
    const requirement = state.outgoing!.requirements[0];
    const original = requirement.requests![0];
    const second = {
      ...original,
      requestId: "reviewer-new",
      state: "pending" as const,
      requestedAt: "2026-10-22T00:00:00Z",
      closedAt: null,
    };
    const old = {
      ...original,
      requestId: "reviewer-old",
      state: "pending" as const,
      requestedAt: "2026-10-20T00:00:00Z",
      closedAt: null,
    };
    requirement.requests = [old, ...requirement.requests!, second];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const timeline = await openApprovalTimeline();
    const group = within(timeline).getByRole("region", {
      name: `Timeline for reviewer ${original.actor!.displayName}`,
    });
    const cards = within(group).getAllByTestId(
      "workspace-approvals-history-card",
    );
    expect(cards[0].querySelector("time")).toHaveAttribute(
      "datetime",
      second.requestedAt,
    );
    expect(cards.at(-1)!.querySelector("time")).not.toHaveAttribute(
      "datetime",
      second.requestedAt,
    );
    expect(
      within(timeline).getAllByTestId("approval-reviewer-timeline"),
    ).toHaveLength(2);
  });

  it("folds legacy approvals below current requirements and retains their timelines and filters while folded", async () => {
    const state = forkState();
    const previous = state.outgoing!.requirements[0];
    state.outgoing!.history = [
      {
        definitionId: previous.definition.id,
        definitionName: previous.definition.name,
        requests: previous.requests!,
        decisionLog: previous.decisionLog,
      },
      {
        definitionId: "never-requested",
        definitionName: "Unused definition",
        requests: [],
        decisionLog: [],
      },
    ];
    state.outgoing!.requirements = [
      {
        ...previous,
        definition: {
          ...previous.definition,
          id: "current",
          name: "Current requirement",
        },
        requests: [],
        approvers: [],
        decisionLog: [],
        callerCanDecide: false,
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    const first = renderTab();
    const legacy = await screen.findByTestId("workspace-approvals-legacy");
    const toggle = within(legacy).getByTestId(
      "workspace-approvals-legacy-toggle",
    );
    expect(toggle).toHaveTextContent("Legacy approvals (1)");
    expect(legacy).not.toHaveAttribute("open");
    const current = screen.getAllByTestId("workspace-approvals-requirement");
    expect(current).toHaveLength(1);
    expect(current[0]).toHaveTextContent("Current requirement");
    expect(legacy).not.toHaveTextContent("Unused definition");
    expect(legacy).not.toHaveTextContent("Current requirement");
    expect(
      within(legacy).getByTestId("workspace-approvals-historical-requirement"),
    ).not.toBeVisible();
    const timeline = await openApprovalTimeline(previous.definition.name);
    expect(legacy).toHaveAttribute("open");
    expect(timeline).toBeVisible();
    await userEvent.click(
      within(timeline).getByRole("checkbox", { name: "Rejected" }),
    );
    expect(timeline).toHaveTextContent("Showing 1 of 2 timeline entries");
    await userEvent.click(toggle);
    expect(legacy).not.toHaveAttribute("open");
    expect(timeline).not.toBeVisible();
    await userEvent.click(toggle);
    expect(timeline).toBeVisible();
    expect(timeline).toHaveTextContent("Showing 1 of 2 timeline entries");
    first.unmount();
    renderTab();
    expect(
      await screen.findByTestId("workspace-approvals-legacy"),
    ).not.toHaveAttribute("open");
  });

  it("retains decisions without request records in their approval's timeline and labels missing request state", async () => {
    const state = forkState();
    const requirement = state.outgoing!.requirements[0];
    state.outgoing!.history = [
      {
        definitionId: requirement.definition.id,
        definitionName: requirement.definition.name,
        requests: requirement.requests!,
        decisionLog: requirement.decisionLog,
      },
      {
        definitionId: "legacy-only",
        definitionName: "Archived approval",
        requests: [],
        decisionLog: [
          {
            ...requirement.decisionLog[0],
            requestId: null,
            outcome: "approved",
            definitionVersion: 1,
            comment: "Retained before request tracking",
            superseded: true,
          },
        ],
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const timeline = await openApprovalTimeline("Archived approval");
    expect(
      within(timeline).queryAllByTestId("workspace-approvals-history-card"),
    ).toHaveLength(0);
    expect(
      within(timeline).getAllByTestId("workspace-approvals-decision-record"),
    ).toHaveLength(1);
    expect(timeline).toHaveTextContent("Approved");
    expect(timeline).toHaveTextContent("Retained before request tracking");
    expect(timeline).toHaveTextContent(
      "Request details unavailable for this retained decision.",
    );
    expect(timeline).toHaveTextContent("Definition version 1");
    expect(timeline).not.toHaveTextContent("No approval requests yet.");
    const status = within(timeline).getByLabelText(
      "Filter approval history by request status",
    );
    await userEvent.selectOptions(status, "Unknown");
    expect(
      within(timeline).getByTestId("workspace-approvals-decision-record"),
    ).toBeVisible();
    await userEvent.selectOptions(status, "Settled");
    expect(
      within(timeline).queryByTestId("workspace-approvals-decision-record"),
    ).toBeNull();
    expect(timeline).toHaveTextContent("No history matches these filters.");
    await userEvent.click(
      within(timeline).getByRole("button", { name: "Clear filters" }),
    );
    expect(
      within(timeline).getByTestId("workspace-approvals-decision-record"),
    ).toBeVisible();
  });

  it("renders a superseded request as its own immutable card", async () => {
    const state = forkState();
    const requirement = state.outgoing!.requirements[0];
    state.outgoing!.history = [
      {
        definitionId: requirement.definition.id,
        definitionName: requirement.definition.name,
        requests: requirement.requests ?? [],
        decisionLog: requirement.decisionLog,
      },
      {
        definitionId: "apprdef-legacy",
        definitionName: "Legacy release review",
        requests: [
          {
            requestId: "superseded-request",
            actorType: "human",
            actorUserId: "user-alex",
            actor: { userId: "user-alex", displayName: "Alex Chen" },
            requestedBy: {
              userId: "user-jordan",
              displayName: "Jordan Rivera",
            },
            state: "superseded",
            definitionVersion: 1,
            definitionName: "Legacy release review",
            requestedAt: "2026-08-10T09:00:00.000Z",
            closedAt: "2026-08-10T10:00:00.000Z",
            agentReviews: [],
          },
        ],
        decisionLog: [],
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();

    const feed = await openApprovalTimeline("Legacy release review");
    const supersededCard = within(feed)
      .getAllByTestId("workspace-approvals-history-card")
      .find((card) => card.textContent?.includes("Legacy release review"))!;
    expect(supersededCard).toHaveTextContent("Superseded");
  });

  it("keeps one immutable card for each request in a re-request chain", async () => {
    const state = forkState();
    const definition = state.outgoing!.requirements[0].definition;
    const chainRequest = (
      requestId: string,
      requestedAt: string,
      closedAt: string | null,
    ) => ({
      requestId,
      actorType: "human" as const,
      actorUserId: "user-alex",
      actor: { userId: "user-alex", displayName: "Alex Chen" },
      requestedBy: { userId: "user-jordan", displayName: "Jordan Rivera" },
      state: closedAt ? ("settled" as const) : ("pending" as const),
      definitionVersion: 1,
      definitionName: definition.name,
      requestedAt,
      closedAt,
      agentReviews: [],
    });
    // request → decision (settles it) → re-request, all on one definition.
    state.outgoing!.history = [
      {
        definitionId: definition.id,
        definitionName: definition.name,
        requests: [
          chainRequest(
            "chain-request-1",
            "2026-08-23T10:00:00.000Z",
            "2026-08-23T11:00:00.000Z",
          ),
          chainRequest("chain-request-2", "2026-08-24T09:00:00.000Z", null),
        ],
        decisionLog: [
          {
            requestId: "chain-request-1",
            outcome: "approved",
            comment: "First pass approved.",
            actor: { userId: "user-alex", displayName: "Alex Chen" },
            definitionVersion: 1,
            superseded: false,
            reviewedGitHeads: [],
            createdAt: "2026-08-23T11:00:00.000Z",
          },
        ],
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();

    const feed = await openApprovalTimeline();
    const cards = within(feed).getAllByTestId(
      "workspace-approvals-history-card",
    );
    expect(cards).toHaveLength(2);
    expect(cards[0]).toHaveTextContent("Awaiting decision");
    expect(cards[1]).toHaveTextContent("Approved");
    expect(within(feed).getByText("First pass approved.")).toBeInTheDocument();
  });
  it("shows three equally prominent sections, per-approval info and folded Reviews", async () => {
    const state = forkState();
    const r = state.outgoing!.requirements[0];
    r.requests = [];
    r.decisionLog = [];
    state.outgoing!.requirements = [
      r,
      {
        ...r,
        definition: {
          ...r.definition,
          id: "alpha",
          name: "Alphabetical approval",
        },
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const requirements = await screen.findAllByTestId(
      "workspace-approvals-requirement",
    );
    expect(requirements[0]).toHaveTextContent("Alphabetical approval");
    await screen.findByTestId("approvals-reviews-toggle");
    // The personal request banner leads the page above the three sections.
    const banner = screen.getByTestId("personal-approvals-banner");
    const headings = screen
      .getAllByRole("heading", { level: 2 })
      .filter((heading) => !banner.contains(heading));
    expect(headings.map((heading) => heading.textContent)).toEqual([
      "Approvals Required to merge into Base",
      expect.stringContaining("Reviews"),
      "Configure Approvals Required to merge into this workspace",
    ]);
    for (const heading of headings)
      expect(heading).toHaveClass("text-xl", "font-semibold");
    expect(
      screen.queryByRole("heading", { name: "Approval history" }),
    ).toBeNull();
    expect(
      screen.queryByTestId("workspace-approvals-advanced-toggle"),
    ).toBeNull();
    expect(
      screen.getByTestId("workspace-approvals-required-into"),
    ).toBeVisible();
    // An untouched, never-requested requirement has no Timeline and no path.
    expect(
      within(requirements[0]).queryByRole("button", {
        name: "Timeline for Alphabetical approval",
      }),
    ).toBeNull();
    expect(
      within(requirements[0]).queryByTestId("workspace-approvals-path"),
    ).toBeNull();
    const toggle = await screen.findByTestId("approvals-reviews-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveTextContent(
      /Reviews.*open review items.*published reviews/,
    );
    const name = within(requirements[0]).getByRole("button", {
      name: "Alphabetical approval",
    });
    expect(name).not.toHaveTextContent("↗");
    await userEvent.click(name);
    const definition = screen.getByRole("dialog", {
      name: "Alphabetical approval definition",
    });
    expect(definition).toHaveTextContent(
      "Required for merges into the production base workspace.",
    );
    // × closes and returns focus to the name.
    await userEvent.click(
      within(definition).getByRole("button", { name: "Close" }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(name).toHaveFocus();
    // Escape closes.
    await userEvent.click(name);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    // A click outside the popup closes it and keeps the page in place.
    await userEvent.click(name);
    await userEvent.click(
      screen.getByTestId("workspace-approvals-definition-popup-overlay"),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(requirements[0]).toBeVisible();
  });
  it("sections the definition popup into Description, Approvers and Prompt", async () => {
    const state = forkState();
    const r = state.outgoing!.requirements[0];
    r.definition = {
      ...r.definition,
      agentType: "simple",
      agentPrompt: "Check the migration is reversible.",
    };
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const [requirement] = await screen.findAllByTestId(
      "workspace-approvals-requirement",
    );
    await userEvent.click(
      within(requirement).getByRole("button", { name: r.definition.name }),
    );
    const definition = screen.getByRole("dialog", {
      name: `${r.definition.name} definition`,
    });
    const headings = within(definition).getAllByRole("heading", { level: 3 });
    expect(headings.map((h) => h.textContent)).toEqual([
      "Description:",
      "Approvers:",
      "Prompt:",
    ]);
    for (const heading of headings)
      expect(heading).toHaveClass("font-semibold");
    expect(
      within(definition).getByRole("region", { name: "Description:" }),
    ).toHaveTextContent(
      "Required for merges into the production base workspace.",
    );
    const agentLine = within(definition).getByTestId(
      "workspace-approvals-definition-agent",
    );
    expect(within(agentLine).getByRole("img", { name: "Agent" })).toBeVisible();
    expect(agentLine).toHaveTextContent(`${r.definition.name} agent`);
    const humansLine = within(definition).getByTestId(
      "workspace-approvals-definition-humans",
    );
    expect(within(humansLine).getAllByRole("img")).toHaveLength(1);
    expect(
      within(humansLine).getByRole("img", { name: "Human" }),
    ).toBeVisible();
    expect(humansLine).toHaveTextContent("Alex Chen, Jordan Rivera");
    expect(
      within(definition).getByRole("region", { name: "Prompt:" }),
    ).toHaveTextContent("Check the migration is reversible.");
    expect(within(definition).getByText(/Self approval/)).toHaveClass(
      "text-xs",
    );
  });
  it("keeps the last decision beside a newer Waiting request", async () => {
    const state = forkState();
    const r = state.outgoing!.requirements[0];
    r.decisionLog = [
      {
        ...r.decisionLog[0],
        actorType: "human",
        actor: { userId: "user-alex", displayName: "Alex Chen" },
        outcome: "approved",
        reviewedGitHeads: activeFlow.heads,
      },
    ];
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const requirement = await screen.findByTestId(
      "workspace-approvals-requirement",
    );
    expect(within(requirement).getByText("Approved")).toBeVisible();
    expect(within(requirement).getByText("Waiting")).toBeVisible();
    expect(
      await within(requirement).findByRole("button", {
        name: "Reviewed commits: matching",
      }),
    ).toBeVisible();
  });
  it("keeps rejection currentness independent of outcome and opens commit details", async () => {
    const state = forkState();
    state.outgoing!.requirements[0].decisionLog[0].reviewedGitHeads =
      activeFlow.heads;
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const requirement = await screen.findByTestId(
      "workspace-approvals-requirement",
    );
    expect(within(requirement).getByText("Rejected")).toBeVisible();
    // The decision-level indicator shares the aggregate's circled family.
    const matching = await within(requirement).findByRole("button", {
      name: "Reviewed commits: matching",
    });
    expect(matching).toHaveTextContent(/^=$/);
    expect(matching).toHaveClass("text-[var(--g5-info)]");
    await userEvent.click(matching);
    const dialog = within(
      screen.getByRole("dialog", { name: "Reviewed and current commits" }),
    );
    expect(dialog.getAllByText("a".repeat(40))).toHaveLength(2);
    expect(dialog.getByText(/not recorded with this decision/)).toBeVisible();
  });
  it("opens the exact agent decision report while showing a different active run", async () => {
    const state = forkState();
    const r = state.outgoing!.requirements[0];
    r.definition.agentType = "simple";
    r.decisionLog = [
      {
        ...r.decisionLog[0],
        actorType: "agent",
        actor: null,
        requestId: "old-request",
        agentReviewId: "old-review",
        outcome: "approved",
      },
    ];
    r.requests = [
      {
        ...r.requests![0],
        actorType: "agent",
        actorUserId: null,
        actor: null,
        requestId: "new-request",
        agentReviews: [
          {
            taskId: null,
            reviewId: "new-review",
            sessionId: null,
            state: "running",
            terminalReason: null,
            createdAt: "2026-10-21T00:00:00Z",
            startedAt: null,
            updatedAt: "2026-10-21T00:00:00Z",
          },
        ],
      },
    ];
    const reads: string[] = [];
    server.use(
      http.get(
        "*/api/v1/workspaces/:id/approvals/reviews/:review/report",
        ({ params }) => {
          reads.push(String(params.review));
          return HttpResponse.json({
            data: {
              document:
                "---\nschemaVersion: 3\n---\n# Exact old report\n\n**Readable finding**\n\n| Repository | Outcome |\n|---|---|\n| app | Approved |",
              sha256: "x",
            },
          });
        },
      ),
    );
    seedMockWorkspaceApprovals(WS_ID, state);
    renderTab();
    const requirement = await screen.findByTestId(
      "workspace-approvals-requirement",
    );
    expect(within(requirement).getByText("Waiting")).toBeVisible();
    const action = within(requirement).getByRole("button", {
      name: "Read report",
    });
    expect(reads).toEqual([]);
    await userEvent.click(action);
    const modal = await screen.findByRole("dialog", {
      name: "Agent review report",
    });
    await within(modal).findByText("Readable finding");
    await userEvent.click(within(modal).getByRole("button", { name: "Raw" }));
    expect(
      within(modal).getByTestId("approval-report-source"),
    ).toHaveTextContent("schemaVersion: 3");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(action).toHaveFocus();
    expect(within(requirement).getByText("Waiting")).toBeVisible();
    expect(reads).toEqual(["old-review"]);
  });
  it("clears open report context when a refreshed decision binds another exact run", async () => {
    const state = forkState();
    const requirement = state.outgoing!.requirements[0];
    requirement.decisionLog = [
      {
        ...requirement.decisionLog[0],
        actorType: "agent",
        actor: null,
        requestId: "same-request",
        agentReviewId: "first-run",
      },
    ];
    server.use(
      http.get(
        "*/api/v1/workspaces/:id/approvals/reviews/:review/report",
        ({ params }) =>
          HttpResponse.json({
            data: {
              assignment: {
                resolution: {
                  targetWorkspaceId: params.review,
                  targetHeads: [],
                },
              },
            },
          }),
      ),
    );
    seedMockWorkspaceApprovals(WS_ID, state);
    const view = renderTab();
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("approval-version-indicator"));
    expect(
      await screen.findByText("Target workspace: first-run"),
    ).toBeVisible();
    const popup = screen.getByRole("dialog", {
      name: "Reviewed and current commits",
    });
    const withOtherReviewer = structuredClone(state);
    withOtherReviewer.outgoing!.requirements[0].decisionLog.unshift({
      ...requirement.decisionLog[0],
      actorType: "human",
      actor: { userId: "another-reviewer", displayName: "Another reviewer" },
      requestId: "unrelated-request",
      agentReviewId: null,
    });
    act(() => {
      view.queryClient.setQueryData(
        ["workspace-approvals", WS_ID],
        withOtherReviewer,
      );
    });
    await waitFor(() =>
      expect(screen.getByText("Another reviewer")).toBeVisible(),
    );
    expect(
      screen.getByRole("dialog", { name: "Reviewed and current commits" }),
    ).toBe(popup);
    expect(screen.getByText("Target workspace: first-run")).toBeVisible();
    const next = structuredClone(withOtherReviewer);
    next.outgoing!.requirements[0].decisionLog[1].agentReviewId = "second-run";
    act(() => {
      view.queryClient.setQueryData(["workspace-approvals", WS_ID], next);
    });
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Reviewed and current commits" }),
      ).not.toBeInTheDocument(),
    );
    const agentRow = screen
      .getAllByTestId("workspace-approvals-decision")
      .find((row) =>
        within(row).queryByText(`${requirement.definition.name} agent`),
      )!;
    await user.click(
      within(agentRow).getByTestId("approval-version-indicator"),
    );
    expect(
      await screen.findByText("Target workspace: second-run"),
    ).toBeVisible();
    expect(
      screen.queryByText("Target workspace: first-run"),
    ).not.toBeInTheDocument();
  });
  it("exposes the unified Request only to the current owner, including human-only requirements", async () => {
    const state = forkState();
    state.outgoing!.requirements[0].requests = [];
    state.outgoing!.requirements[0].decisionLog = [];
    seedMockWorkspaceApprovals(WS_ID, state);
    seedFormalReviews(true);
    const first = renderTab();
    expect(await screen.findByTestId("workspace-request-review")).toBeVisible();
    first.unmount();
    server.use(
      http.get(`/api/v1/workspaces/${WS_ID}`, () =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails.base,
            id: WS_ID,
            ownerUserId: "another-owner",
            isBase: false,
          },
        }),
      ),
    );
    renderTab({ role: "admin" });
    await screen.findByTestId("workspace-approvals-outgoing");
    expect(screen.queryByTestId("workspace-request-review")).toBeNull();
  });
  it("opens Reviews for a review deep link and keeps feedback reachable", async () => {
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab({ search: "?view=reviews" });
    const toggle = await screen.findByTestId("approvals-reviews-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByRole("button", { name: "Add workspace comment" }),
    ).toBeVisible();
  });
  it("keeps the sole comment action beside folded Reviews and reveals its threads when composing", async () => {
    seedMockWorkspaceApprovals(WS_ID, forkState());
    renderTab();
    const toggle = await screen.findByTestId("approvals-reviews-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    const actions = screen.getAllByRole("button", {
      name: "Add workspace comment",
    });
    expect(actions).toHaveLength(1);
    expect(actions[0]).toBeVisible();
    await waitFor(() => expect(actions[0]).toBeEnabled());
    await userEvent.click(actions[0]);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByRole("dialog", { name: "Add workspace comment" }),
    ).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Workspace comments" }),
    ).toBeVisible();
  });
  it.each([
    { label: "summary-only", count: 0, status: "open", open: 0 },
    { label: "three unresolved comments", count: 3, status: "open", open: 3 },
    { label: "three resolved comments", count: 3, status: "resolved", open: 0 },
  ])(
    "distinguishes one published review from its items: $label",
    async ({ count, status, open }) => {
      seedMockWorkspaceApprovals(WS_ID, forkState());
      server.use(
        http.get(`/api/v1/workspaces/${WS_ID}/review/publications`, () =>
          HttpResponse.json({
            data: {
              publications: [
                {
                  id: "published",
                  orgId: ORG_ID,
                  sourceWorkspaceId: WS_ID,
                  reviewWorkspaceId: null,
                  reviewerUserId: "user-jordan",
                  authorKind: "human",
                  authorLabel: "Jordan",
                  round: 1,
                  kind: "round",
                  baseCommitVector: [],
                  summary: "Overall feedback",
                  verdict: "comment_only",
                  scores: null,
                  supersededBy: null,
                  createdAt: "2026-10-23T00:00:00Z",
                  openCount: open,
                  acceptedCount: 0,
                  rejectedCount: 0,
                  resolvedCount: count - open,
                  supersededCount: 0,
                  artifactCount: 0,
                },
              ],
              findings: [],
              reviewerDisplayNames: {},
              sourceHeads: {},
            },
          }),
        ),
        http.get(`/api/v1/workspaces/${WS_ID}/review/suggestions`, () =>
          HttpResponse.json({
            data: {
              suggestions: Array.from({ length: count }, (_, index) => ({
                id: `item-${index}`,
                publicationId: "published",
                findingId: null,
                repoId: "code",
                path: "docs/design.md",
                kind: "comment",
                beforeLines: null,
                afterLines: null,
                anchorStart: null,
                contentHash: null,
                anchorContext: null,
                authorKind: "human",
                authorLabel: "Jordan",
                note: `Review comment ${index}`,
                commentId: null,
                status,
                statusBy: null,
                statusAt: null,
                supersedes: null,
                adoptedFrom: null,
                createdAt: "2026-10-23T00:00:00Z",
              })),
            },
          }),
        ),
      );
      renderTab({ search: "?view=reviews" });
      const toggle = await screen.findByTestId("approvals-reviews-toggle");
      await waitFor(() =>
        expect(toggle).toHaveTextContent(
          `${open} open review item${open === 1 ? "" : "s"} · 1 published review`,
        ),
      );
      const explanation = screen.getByText(
        /A published review can have zero open items/,
      );
      expect(explanation).not.toBeVisible();
      await userEvent.click(
        screen.getByLabelText("About reviews and comments"),
      );
      expect(explanation).toBeVisible();
      expect(
        screen.getByText(/are general feedback entered here/),
      ).toBeVisible();
      screen.getByLabelText("About reviews and comments").focus();
      await userEvent.keyboard("{Escape}");
      expect(explanation).not.toBeVisible();
      expect(
        screen.getAllByRole("button", {
          name: "Add workspace comment",
        }),
      ).toHaveLength(1);
    },
  );
  it("permits an addressed reader to decide while hiding copy creation without its existing writer scope", async () => {
    seedMockWorkspaceApprovals(WS_ID, forkState());
    server.use(
      http.get(`/api/v1/workspaces/${WS_ID}`, () =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails.base,
            id: WS_ID,
            ownerUserId: "user-jordan",
            isBase: false,
            readOnly: false,
          },
        }),
      ),
    );
    renderTab({ role: "member", scopes: ["workspaces:read"] });
    expect(
      await screen.findByRole("button", { name: "Approve" }),
    ).toBeEnabled();
    expect(screen.queryByTestId("workspace-request-review")).toBeNull();
    expect(screen.queryByTestId("review-workspace-start")).toBeNull();
  });

  describe("named requirements (#974)", () => {
    const person = (userId: string, displayName: string) => ({
      userId,
      displayName,
      email: `${userId}@g5labs.dev`,
      avatarUrl: null,
      stillMember: true,
    });
    const request = (
      over: Partial<ApprovalRequestDTO> & { requestId: string },
    ): ApprovalRequestDTO => ({
      actorType: "human",
      actorUserId: null,
      actor: null,
      requestedBy: { userId: "user-alex", displayName: "Alex Chen" },
      state: "settled",
      definitionVersion: 1,
      definitionName: "",
      requestedAt: "2026-10-30T09:00:00.000Z",
      closedAt: null,
      agentReviews: [],
      ...over,
    });
    const movedHeads = () => [
      { ...activeFlow.heads[0], headCommit: "c".repeat(40) },
      activeFlow.heads[1],
    ];
    function mixedState(): WorkspaceApprovalsStateDTO {
      const base = forkState();
      const docs = makeDefinition({
        id: "docs",
        name: "Documentation",
        requireAllReviewers: false,
        humans: [
          person("user-kaihui", "Kaihui Zheng"),
          person("user-lena", "Lena Ortiz"),
        ],
      });
      const security = makeDefinition({
        id: "security",
        name: "Security review",
        requireAllReviewers: true,
        agentType: "simple",
        humans: [person("user-ben", "Ben"), person("user-tim", "Tim Kraska")],
      });
      const architecture = makeDefinition({
        id: "architecture",
        name: "Architecture",
        requireAllReviewers: false,
        agentType: "simple",
        humans: [person("user-ben", "Ben")],
      });
      const ui = makeDefinition({
        id: "ui",
        name: "UI changes",
        requireAllReviewers: false,
        agentType: "simple",
        humans: [person("user-ben", "Ben")],
      });
      const none = { agent: false, humans: [], randomHumanFrom: [] };
      base.outgoing!.requirements = [
        {
          definition: security,
          state: "pending",
          approvers: [],
          callerCanDecide: false,
          currentness: {
            state: "changed",
            notCurrent: [
              {
                actorType: "agent",
                userId: null,
                displayName: "Security review agent",
              },
            ],
          },
          approvalPath: {
            kind: "needed",
            approvers: [
              {
                actorType: "human",
                userId: "user-tim",
                displayName: "Tim Kraska",
              },
            ],
            requireAll: true,
            allHumansRequired: true,
          },
          requestNeeds: {
            outstanding: { ...none, humans: ["user-tim"] },
            repeat: none,
            agentRequestable: false,
            agentApproved: true,
          },
          requests: [
            request({
              requestId: "sec-agent-0",
              actorType: "agent",
              definitionName: "Security review",
              state: "cancelled",
              requestedAt: "2026-10-28T09:00:00.000Z",
            }),
            request({
              requestId: "sec-agent-1",
              actorType: "agent",
              definitionName: "Security review",
              requestedAt: "2026-10-29T09:00:00.000Z",
            }),
            request({
              requestId: "sec-agent-2",
              actorType: "agent",
              definitionName: "Security review",
              state: "pending",
              requestedAt: "2026-10-30T09:05:00.000Z",
              agentReviews: [
                {
                  taskId: "task-2",
                  reviewId: "review-2",
                  sessionId: null,
                  state: "running",
                  terminalReason: null,
                  createdAt: "2026-10-30T09:05:00.000Z",
                  startedAt: null,
                  updatedAt: "2026-10-30T09:05:00.000Z",
                },
              ],
            }),
            request({
              requestId: "sec-ben",
              actorUserId: "user-ben",
              actor: { userId: "user-ben", displayName: "Ben" },
              definitionName: "Security review",
            }),
            request({
              requestId: "sec-tim",
              actorUserId: "user-tim",
              actor: { userId: "user-tim", displayName: "Tim Kraska" },
              definitionName: "Security review",
            }),
          ],
          decisionLog: [
            {
              requestId: "sec-agent-1",
              agentReviewId: "review-1",
              actorType: "agent",
              actor: null,
              outcome: "approved",
              comment: null,
              summary: "The authorization checks passed.",
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: movedHeads(),
              createdAt: "2026-10-29T22:45:00.000Z",
            },
            {
              requestId: "sec-ben",
              actorType: "human",
              actor: { userId: "user-ben", displayName: "Ben" },
              outcome: "approved",
              comment: "The authorization change is acceptable.",
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: activeFlow.heads,
              createdAt: "2026-10-30T09:06:00.000Z",
            },
            {
              requestId: "sec-tim",
              actorType: "human",
              actor: { userId: "user-tim", displayName: "Tim Kraska" },
              outcome: "deferred",
              comment: "Withdrawing until the follow-up is reviewed.",
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: activeFlow.heads,
              createdAt: "2026-10-30T09:07:00.000Z",
            },
          ],
        },
        {
          definition: docs,
          state: "rejected",
          approvers: [],
          callerCanDecide: false,
          currentness: { state: "matching", notCurrent: [] },
          approvalPath: {
            kind: "humansRejected",
            humans: [
              {
                actorType: "human",
                userId: "user-kaihui",
                displayName: "Kaihui Zheng",
              },
            ],
            othersRequired: false,
          },
          requestNeeds: {
            outstanding: { ...none, humans: ["user-kaihui"] },
            repeat: none,
            agentRequestable: false,
            agentApproved: false,
          },
          requests: [
            request({
              requestId: "docs-kaihui",
              actorUserId: "user-kaihui",
              actor: { userId: "user-kaihui", displayName: "Kaihui Zheng" },
              definitionName: "Documentation",
            }),
            request({
              requestId: "docs-lena",
              actorUserId: "user-lena",
              actor: { userId: "user-lena", displayName: "Lena Ortiz" },
              definitionName: "Documentation",
              state: "pending",
              requestedBy: { userId: "user-jordan", displayName: "Jordan" },
            }),
          ],
          decisionLog: [
            {
              requestId: "docs-kaihui",
              actorType: "human",
              actor: { userId: "user-kaihui", displayName: "Kaihui Zheng" },
              outcome: "rejected",
              comment: "The instructions still describe the old button.",
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: activeFlow.heads,
              createdAt: "2026-10-30T09:06:00.000Z",
            },
          ],
        },
        {
          definition: architecture,
          state: "approved",
          approvers: [],
          callerCanDecide: false,
          currentness: { state: "unknown", notCurrent: [] },
          approvalPath: null,
          requestNeeds: {
            outstanding: none,
            repeat: { ...none, agent: true },
            agentRequestable: true,
            agentApproved: true,
          },
          requests: [
            request({
              requestId: "arch-agent",
              actorType: "agent",
              definitionName: "Architecture",
            }),
          ],
          decisionLog: [
            {
              requestId: "arch-agent",
              agentReviewId: "arch-review",
              actorType: "agent",
              actor: null,
              outcome: "approved",
              comment: null,
              summary: null,
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: null,
              createdAt: "2026-10-30T08:20:00.000Z",
            },
          ],
        },
        {
          definition: ui,
          state: "pending",
          approvers: [],
          callerCanDecide: false,
          currentness: { state: "none", notCurrent: [] },
          approvalPath: null,
          requestNeeds: {
            outstanding: { ...none, agent: true },
            repeat: { ...none, agent: true },
            agentRequestable: true,
            agentApproved: false,
          },
          requests: [],
          decisionLog: [],
        },
      ];
      return base;
    }
    async function requirementNamed(name: string) {
      const all = await screen.findAllByTestId(
        "workspace-approvals-requirement",
      );
      return all.find((el) => within(el).queryByRole("button", { name }))!;
    }

    it("shows alphabetical requirements with the server status, path and aggregate currentness", async () => {
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const requirements = await screen.findAllByTestId(
        "workspace-approvals-requirement",
      );
      expect(
        requirements.map(
          (r) =>
            within(r).getByTestId("workspace-approvals-definition").textContent,
        ),
      ).toEqual([
        "Architecture",
        "Documentation",
        "Security review",
        "UI changes",
      ]);
      expect(
        requirements.map(
          (r) =>
            within(r).getByTestId("workspace-approvals-requirement-status")
              .textContent,
        ),
      ).toEqual(["Approved", "Rejected", "Pending", "Pending"]);
      const [architecture, docs, security, ui] = requirements;
      expect(
        within(docs).getByTestId("workspace-approvals-path"),
      ).toHaveTextContent(
        "Rejected by Kaihui Zheng. Kaihui Zheng has to approve (with no other human rejection) to meet this requirement.",
      );
      expect(
        within(security).getByTestId("workspace-approvals-path").textContent,
      ).toBe(
        "Approval from Tim Kraska will meet this requirement. All configured humans have to approve to meet this requirement.",
      );
      expect(
        within(architecture).queryByTestId("workspace-approvals-path"),
      ).toBeNull();
      expect(within(ui).queryByTestId("workspace-approvals-path")).toBeNull();
      // One aggregate icon per requirement with counted evidence.
      expect(
        within(ui).queryByTestId("workspace-approvals-currentness"),
      ).toBeNull();
      // One circled glyph family (#1095): = blue, ≠ amber, ? gray.
      const cases: [HTMLElement, string, string, string, string][] = [
        [
          architecture,
          "Source commits unknown",
          "Source commits unknown.",
          "?",
          "var(--g5-text2)",
        ],
        [
          docs,
          "All approvals are current",
          "All approvals are current.",
          "=",
          "var(--g5-info)",
        ],
        [
          security,
          "Some approvals are not current",
          "Approvals from Security review agent are not current.",
          "≠",
          "var(--g5-warn)",
        ],
      ];
      for (const [element, label, message, glyph, tone] of cases) {
        const icon = within(element).getByRole("button", {
          name: `Overall commit status: ${label}`,
        });
        expect(icon.textContent).toBe(glyph);
        expect(icon).toHaveClass(`text-[${tone}]`);
        expect(icon.querySelector("span")).toHaveClass("rounded-full");
        await userEvent.click(icon);
        const popup = screen.getByRole("dialog", {
          name: /approval commit status$/,
        });
        expect(popup).toHaveTextContent(message);
        await userEvent.click(
          within(popup).getByRole("button", { name: "Close" }),
        );
        expect(screen.queryByRole("dialog")).toBeNull();
        expect(icon).toHaveFocus();
      }
    });

    it.each([
      [
        {
          kind: "agentRejected",
          agentName: "UI changes agent",
          humanOverride: true,
        },
        "Rejected by UI changes agent. A new agent approval, or a configured human approval with no human rejection, will meet this requirement.",
      ],
      [
        {
          kind: "agentRejected",
          agentName: "UI changes agent",
          humanOverride: false,
        },
        "Rejected by UI changes agent. A new agent approval will meet this requirement.",
      ],
      [
        {
          kind: "humansRejected",
          humans: [
            { actorType: "human", userId: "a", displayName: "Ann" },
            { actorType: "human", userId: "b", displayName: "Bo" },
          ],
          othersRequired: true,
        },
        "Rejected by Ann and Bo. Ann and Bo have to approve (with no other human rejection) to meet this requirement. Additionally, all other configured humans also have to approve to meet this requirement.",
      ],
      [
        {
          kind: "needed",
          approvers: [
            {
              actorType: "agent",
              userId: null,
              displayName: "UI changes agent",
            },
            { actorType: "human", userId: "b", displayName: "Ben" },
          ],
          requireAll: false,
          allHumansRequired: false,
        },
        "An approval from UI changes agent or Ben will meet this requirement.",
      ],
    ] as const)(
      "words the path to approval exactly (%#)",
      async (path, text) => {
        const state = mixedState();
        const ui = state.outgoing!.requirements.find(
          (r) => r.definition.name === "UI changes",
        )!;
        ui.state = path.kind === "needed" ? "pending" : "rejected";
        ui.approvalPath = structuredClone(path) as typeof ui.approvalPath;
        seedMockWorkspaceApprovals(WS_ID, state);
        renderTab();
        const requirement = await requirementNamed("UI changes");
        expect(
          within(requirement).getByTestId("workspace-approvals-path")
            .textContent,
        ).toBe(text);
      },
    );

    it("lists only requested approvers, agent first, with type icons, summaries and an earlier settled response", async () => {
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const security = await requirementNamed("Security review");
      const roster = within(security).getByRole("list", {
        name: "Requested approvers for Security review",
      });
      const rows = within(roster).getAllByRole("listitem");
      expect(
        rows.map((row) => row.querySelector(".font-semibold")?.textContent),
      ).toEqual(["Security review agent", "Ben", "Tim Kraska"]);
      expect(within(rows[0]).getByRole("img", { name: "Agent" })).toBeVisible();
      expect(within(rows[1]).getByRole("img", { name: "Human" })).toBeVisible();
      // The newest agent request waits; its earlier settled approval stays
      // beneath it with its own commit evidence and authored summary. The
      // older cancelled attempt is omitted.
      expect(within(rows[0]).getByText("Waiting")).toBeVisible();
      expect(within(rows[0]).queryByText("Cancelled")).toBeNull();
      const prior = within(rows[0]).getByTestId("workspace-approvals-prior");
      expect(prior).toHaveTextContent("Earlier settled response");
      expect(within(prior).getByText("Approved")).toBeVisible();
      expect(
        within(prior).getByText("The authorization checks passed."),
      ).toBeVisible();
      expect(
        await within(prior).findByRole("button", {
          name: "Reviewed commits: changed",
        }),
      ).toBeVisible();
      expect(
        within(prior).queryByRole("button", { name: "Request again" }),
      ).toBeNull();
      // Human summaries are their comments.
      expect(rows[1]).toHaveTextContent(
        "The authorization change is acceptable.",
      );
      expect(within(rows[2]).getByText("Withdrawn")).toBeVisible();
      // No placeholder rows for unrequested humans anywhere.
      expect(screen.queryByText(/Not requested/)).toBeNull();
      // A legacy agent decision without a summary shows no excerpt.
      const architecture = await requirementNamed("Architecture");
      expect(
        within(architecture).queryByTestId("workspace-approvals-summary"),
      ).toBeNull();
      expect(
        within(architecture).getByRole("button", { name: "Read report" }),
      ).toBeVisible();
    });

    it("offers Cancel request only where the viewer may cancel and Request again only beside the latest non-approved response", async () => {
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const security = await requirementNamed("Security review");
      const docs = await requirementNamed("Documentation");
      const architecture = await requirementNamed("Architecture");
      // Agent request: cancellable by the requesting owner.
      expect(
        await within(security).findByRole("button", { name: "Cancel request" }),
      ).toBeVisible();
      // A human request initiated by someone else has no Cancel for this viewer.
      expect(
        within(docs).queryByRole("button", { name: "Cancel request" }),
      ).toBeNull();
      // Rejected (Kaihui) and Withdrawn (Tim) get Request again; Approved,
      // and a prior response under a newer waiting request, do not.
      expect(
        within(docs).getAllByRole("button", { name: "Request again" }),
      ).toHaveLength(1);
      expect(
        within(security).getAllByRole("button", { name: "Request again" }),
      ).toHaveLength(1);
      expect(
        within(architecture).queryByRole("button", { name: "Request again" }),
      ).toBeNull();
      // No large requirement-level re-request buttons remain.
      expect(
        screen.queryByRole("button", { name: /^Request .* again$/ }),
      ).toBeNull();
      // Outstanding approvals sits on non-approved title rows with needs.
      expect(
        within(architecture).queryByRole("button", {
          name: "Request outstanding approvals",
        }),
      ).toBeNull();
      expect(
        within(docs).getByRole("button", {
          name: "Request outstanding approvals",
        }),
      ).toBeVisible();
    });

    it("puts Request again on the response line, else at the end of the decision row", async () => {
      const state = mixedState();
      seedMockWorkspaceApprovals(WS_ID, state);
      const view = renderTab();
      let security = await requirementNamed("Security review");
      const again = () =>
        within(security).getByRole("button", { name: "Request again" });
      // Tim's withdrawal has a comment: the action shares its line.
      expect(again().closest("div")).toHaveTextContent(
        "Reviewer response:Withdrawing until the follow-up is reviewed.",
      );
      expect(again().closest("div")).not.toHaveTextContent("Tim Kraska");

      // Without a response line it stays on the decision row.
      const withdrawal = state
        .outgoing!.requirements.find(
          (r) => r.definition.name === "Security review",
        )!
        .decisionLog.find((d) => d.outcome === "deferred")!;
      withdrawal.comment = null;
      seedMockWorkspaceApprovals(WS_ID, state);
      await view.queryClient.invalidateQueries({
        queryKey: ["workspace-approvals", WS_ID],
      });
      await waitFor(() =>
        expect(within(security).queryByText(/Withdrawing until/)).toBeNull(),
      );
      security = await requirementNamed("Security review");
      expect(again().parentElement?.parentElement).toHaveTextContent(
        /^Tim Kraska\s*Withdrawn/,
      );
    });

    it("hides Request again for a cancelled request without a settled decision", async () => {
      const state = mixedState();
      const ui = state.outgoing!.requirements.find(
        (r) => r.definition.name === "UI changes",
      )!;
      ui.requests = [
        request({
          requestId: "ui-agent",
          actorType: "agent",
          definitionName: "UI changes",
          state: "cancelled",
        }),
      ];
      seedMockWorkspaceApprovals(WS_ID, state);
      renderTab();
      const requirement = await requirementNamed("UI changes");
      expect(within(requirement).getByText("Cancelled")).toBeVisible();
      expect(
        within(requirement).queryByRole("button", { name: "Request again" }),
      ).toBeNull();
    });

    it("opens the shared dialog in Pick mode with exactly the Request again or outstanding recipients checked", async () => {
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const security = await requirementNamed("Security review");
      await userEvent.click(
        within(security).getByRole("button", { name: "Request again" }),
      );
      let dialog = screen.getByRole("dialog", { name: "Request approvals" });
      expect(
        within(dialog).getByTestId("approval-request-mode"),
      ).toHaveTextContent("Pick approvals and approvers");
      expect(
        within(dialog).getByRole("checkbox", {
          name: "Include satisfied approval requirements",
        }),
      ).toBeDisabled();
      const checked = () =>
        within(dialog)
          .getAllByRole("checkbox")
          .filter((box) => (box as HTMLInputElement).checked)
          .map(
            (box) =>
              box.closest("fieldset")?.querySelector("legend")?.textContent +
              " → " +
              box.parentElement?.textContent,
          );
      expect(checked()).toEqual(["Security review → Tim Kraska"]);
      expect(
        within(dialog)
          .getAllByTestId("approval-request-pair")
          .map((p) => p.textContent),
      ).toEqual(["Security review → Tim Kraska"]);
      await userEvent.click(
        within(dialog).getByRole("button", { name: "Close" }),
      );
      expect(screen.queryByRole("dialog")).toBeNull();

      const docs = await requirementNamed("Documentation");
      await userEvent.click(
        within(docs).getByRole("button", {
          name: "Request outstanding approvals",
        }),
      );
      dialog = screen.getByRole("dialog", { name: "Request approvals" });
      expect(checked()).toEqual(["Documentation → Kaihui Zheng"]);
      expect(
        within(dialog).getByTestId("approval-request-selected"),
      ).toBeVisible();
      await userEvent.keyboard("{Escape}");

      // A page-level open stays fresh: no mode and no Selected approvals.
      await userEvent.click(screen.getByTestId("workspace-request-review"));
      dialog = screen.getByRole("dialog", { name: "Request approvals" });
      expect(
        within(dialog).getByTestId("approval-request-mode"),
      ).toHaveTextContent(/^Select▾$/);
      expect(
        within(dialog).queryByTestId("approval-request-selected"),
      ).toBeNull();
      expect(
        within(dialog).getByRole("button", { name: "Review request" }),
      ).toBeDisabled();
    });

    it("reveals Request outstanding approvals after a necessary waiting request is cancelled", async () => {
      const state = mixedState();
      const ui = state.outgoing!.requirements.find(
        (r) => r.definition.name === "UI changes",
      )!;
      const none = { agent: false, humans: [], randomHumanFrom: [] };
      ui.requests = [
        request({
          requestId: "ui-agent",
          actorType: "agent",
          definitionName: "UI changes",
          state: "pending",
        }),
      ];
      ui.requestNeeds = {
        outstanding: none,
        repeat: none,
        agentRequestable: false,
        agentApproved: false,
      };
      seedMockWorkspaceApprovals(WS_ID, state);
      const cancelled: string[] = [];
      server.use(
        http.post(
          `*/api/v1/workspaces/${WS_ID}/approvals/requests/:id/cancel`,
          ({ params }) => {
            cancelled.push(String(params.id));
            const next = structuredClone(state);
            const target = next.outgoing!.requirements.find(
              (r) => r.definition.name === "UI changes",
            )!;
            target.requests![0].state = "cancelled";
            target.requestNeeds = {
              outstanding: { ...none, agent: true },
              repeat: { ...none, agent: true },
              agentRequestable: true,
              agentApproved: false,
            };
            seedMockWorkspaceApprovals(WS_ID, next);
            return HttpResponse.json({ data: next });
          },
        ),
      );
      renderTab();
      const requirement = await requirementNamed("UI changes");
      expect(
        within(requirement).queryByRole("button", {
          name: "Request outstanding approvals",
        }),
      ).toBeNull();
      await userEvent.click(
        await within(requirement).findByRole("button", {
          name: "Cancel request",
        }),
      );
      expect(cancelled).toEqual(["ui-agent"]);
      expect(
        await within(requirement).findByRole("button", {
          name: "Request outstanding approvals",
        }),
      ).toBeVisible();
      expect(within(requirement).getByText("Cancelled")).toBeVisible();
    });

    it("keeps a collapsed indented Timeline with the existing filters and per-review commit evidence", async () => {
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const security = await requirementNamed("Security review");
      const toggle = within(security).getByRole("button", {
        name: "Timeline for Security review",
      });
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(toggle).toHaveTextContent("▶");
      expect(toggle).toHaveTextContent("Timeline");
      await userEvent.click(toggle);
      expect(toggle).toHaveTextContent("▼");
      const timeline = screen.getByRole("region", {
        name: "Timeline for Security review",
      });
      expect(
        within(timeline).getByTestId("workspace-approvals-history-filters"),
      ).toBeVisible();
      expect(
        within(timeline).getAllByTestId("workspace-approvals-technical-details")
          .length,
      ).toBeGreaterThan(0);
      // Every settled review card has its own commit evidence.
      const settled = within(timeline)
        .getAllByTestId("workspace-approvals-history-card")
        .filter((card) =>
          /Approved|Rejected|Decision withdrawn/.test(card.textContent ?? ""),
        );
      expect(settled).toHaveLength(3);
      for (const card of settled)
        expect(
          within(card).getByTestId("approval-version-indicator"),
        ).toBeVisible();
    });

    it("keeps each repository's reviewed/current comparison distinct", async () => {
      server.use(
        http.get(
          "*/api/v1/workspaces/:id/approvals/reviews/:review/report",
          () =>
            HttpResponse.json({
              data: {
                assignment: {
                  resolution: {
                    targetWorkspaceId: "ws-base",
                    targetHeads: [
                      {
                        repoId: "code",
                        branchName: "main",
                        headCommit: "d".repeat(40),
                      },
                    ],
                  },
                },
              },
            }),
        ),
      );
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const security = await requirementNamed("Security review");
      const prior = within(security).getByTestId("workspace-approvals-prior");
      await userEvent.click(
        await within(prior).findByRole("button", {
          name: "Reviewed commits: changed",
        }),
      );
      const popup = screen.getByRole("dialog", {
        name: "Reviewed and current commits",
      });
      expect(popup).toHaveTextContent("Security review agent");
      const rows = within(popup).getAllByTestId("approval-version-repository");
      expect(rows).toHaveLength(2);
      expect(rows[0]).toHaveTextContent("c".repeat(40));
      expect(rows[0]).toHaveTextContent("a".repeat(40));
      expect(rows[0]).toHaveTextContent("Changed");
      expect(rows[1]).toHaveTextContent("Current");
      expect(
        await within(popup).findByText("Target workspace: ws-base"),
      ).toBeVisible();
      await userEvent.click(
        screen.getByTestId("approval-version-popup-overlay"),
      );
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    it("opens the agent report reader over the page and dismisses it with × or an outside click", async () => {
      server.use(
        http.get(
          "*/api/v1/workspaces/:id/approvals/reviews/:review/report",
          () =>
            HttpResponse.json({
              data: {
                document:
                  "---\nschemaVersion: 5\nsummary: Fine.\n---\n# Report",
              },
            }),
        ),
      );
      seedMockWorkspaceApprovals(WS_ID, mixedState());
      renderTab();
      const architecture = await requirementNamed("Architecture");
      const read = within(architecture).getByRole("button", {
        name: "Read report",
      });
      await userEvent.click(read);
      let reader = await screen.findByRole("dialog", {
        name: "Agent review report",
      });
      expect(
        within(reader).getByRole("button", { name: "View" }),
      ).toBeVisible();
      await userEvent.click(
        within(reader).getByRole("button", { name: "Close" }),
      );
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(read).toHaveFocus();
      await userEvent.click(read);
      reader = await screen.findByRole("dialog", {
        name: "Agent review report",
      });
      await userEvent.click(
        screen.getByTestId("approval-review-reader-overlay"),
      );
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(architecture).toBeVisible();
    });

    it("hides row request actions while the request PR rule blocks requests", async () => {
      const state = mixedState();
      state.outgoing!.requestPolicy = {
        prRequired: true,
        onSourceCommitVectorChange: "IGNORE",
      };
      seedMockWorkspaceApprovals(WS_ID, state);
      activeFlow.pr = null;
      renderTab();
      const docs = await requirementNamed("Documentation");
      await waitFor(() =>
        expect(
          within(docs).queryByRole("button", { name: "Request again" }),
        ).toBeNull(),
      );
      expect(
        within(docs).queryByRole("button", {
          name: "Request outstanding approvals",
        }),
      ).toBeNull();
    });
  });

  describe("issue #1095 presentation", () => {
    const person = (userId: string, displayName: string) => ({
      userId,
      displayName,
      email: `${userId}@g5labs.dev`,
      avatarUrl: null,
      stillMember: true,
    });
    const ontology = makeDefinition({
      id: "ontology",
      name: "Ontology Alignment",
      requireAllReviewers: false,
      agentType: "simple",
      humans: [person("user-maya", "Maya Chen"), person("user-ben", "Ben")],
    });
    const request = (
      over: Partial<ApprovalRequestDTO> & { requestId: string },
    ): ApprovalRequestDTO => ({
      actorType: "human",
      actorUserId: null,
      actor: null,
      requestedBy: { userId: "user-alex", displayName: "Alex Chen" },
      state: "pending",
      definitionVersion: 1,
      definitionName: ontology.name,
      requestedAt: "2026-11-04T09:00:00.000Z",
      closedAt: null,
      agentReviews: [],
      ...over,
    });
    const agentRun = request({
      requestId: "agent-request",
      actorType: "agent",
      state: "settled",
      requestedAt: "2026-11-04T08:00:00.000Z",
    });
    const agentRejection = {
      requestId: "agent-request",
      agentReviewId: "run-1",
      actorType: "agent" as const,
      outcome: "rejected" as const,
      comment: null,
      summary: "Rejected: cancellation can leave review resources active.",
      actor: null,
      definitionVersion: 1,
      superseded: false,
      reviewedGitHeads: [],
      createdAt: "2026-11-04T08:20:00.000Z",
    };
    function stateWith(
      over: Partial<WorkspaceOutgoingRequirementDTO>,
    ): WorkspaceApprovalsStateDTO {
      const base = forkState();
      base.outgoing!.requirements = [
        {
          definition: ontology,
          state: "rejected",
          approvers: [],
          callerCanDecide: false,
          requests: [agentRun],
          decisionLog: [agentRejection],
          approvalPath: {
            kind: "agentRejected",
            agentName: "Ontology Alignment agent",
            humanOverride: true,
          },
          ...over,
        },
      ];
      return base;
    }
    async function ontologyRequirement() {
      return within(
        (await screen.findAllByTestId("workspace-approvals-requirement"))[0],
      );
    }
    const path =
      "Rejected by Ontology Alignment agent. A new agent approval, or a configured human approval with no human rejection, will meet this requirement.";

    it("relabels only the requirement and counted agent badges while a human escalation is open", async () => {
      seedMockWorkspaceApprovals(
        WS_ID,
        stateWith({
          agentEscalation: {
            state: "needsHumanEscalation",
            agentReviewId: "run-1",
          },
        }),
      );
      renderTab();
      const requirement = await ontologyRequirement();
      expect(
        requirement.getByTestId("workspace-approvals-requirement-status"),
      ).toHaveTextContent(/^Needs Human Escalation$/);
      const agent = requirement.getByTestId("workspace-approvals-decision");
      expect(agent).toHaveTextContent("Needs Human Escalation");
      expect(agent).not.toHaveTextContent(/\bRejected\b(?!:)/);
      // Path text and the agent's own words are unchanged.
      expect(
        requirement.getByTestId("workspace-approvals-path"),
      ).toHaveTextContent(path);
      expect(within(agent).getByText(agentRejection.summary)).toBeVisible();
    });

    it("reads Overridden by Human beside an Approved requirement, and Rejected without an escalation", async () => {
      const maya = request({
        requestId: "maya-request",
        actorUserId: "user-maya",
        actor: { userId: "user-maya", displayName: "Maya Chen" },
        state: "settled",
      });
      seedMockWorkspaceApprovals(
        WS_ID,
        stateWith({
          state: "approved",
          approvalPath: null,
          agentEscalation: {
            state: "overriddenByHuman",
            agentReviewId: "run-1",
          },
          requests: [agentRun, maya],
          decisionLog: [
            {
              requestId: "maya-request",
              actorType: "human",
              outcome: "approved",
              comment: "Mitigation accepted.",
              actor: { userId: "user-maya", displayName: "Maya Chen" },
              definitionVersion: 1,
              superseded: false,
              reviewedGitHeads: [],
              createdAt: "2026-11-04T09:30:00.000Z",
            },
            agentRejection,
          ],
        }),
      );
      const view = renderTab();
      let requirement = await ontologyRequirement();
      expect(
        requirement.getByTestId("workspace-approvals-requirement-status"),
      ).toHaveTextContent(/^Approved$/);
      const [agent, human] = requirement.getAllByTestId(
        "workspace-approvals-decision",
      );
      expect(agent).toHaveTextContent("Overridden by Human");
      expect(human).toHaveTextContent("Approved");
      expect(human).toHaveTextContent("Reviewer response:Mitigation accepted.");
      expect(agent).toHaveTextContent(
        `Agent response:${agentRejection.summary}`,
      );

      seedMockWorkspaceApprovals(WS_ID, stateWith({ agentEscalation: null }));
      await view.queryClient.invalidateQueries();
      await waitFor(async () => {
        requirement = await ontologyRequirement();
        expect(
          requirement.getByTestId("workspace-approvals-requirement-status"),
        ).toHaveTextContent(/^Rejected$/);
      });
      expect(
        requirement.getByTestId("workspace-approvals-decision"),
      ).toHaveTextContent("Rejected");
    });

    it("makes Read report the prominent action and keeps Request again a text link", async () => {
      seedMockWorkspaceApprovals(WS_ID, stateWith({}));
      renderTab();
      const requirement = await ontologyRequirement();
      const read = requirement.getByRole("button", { name: "Read report" });
      expect(read).toHaveClass("font-semibold", "bg-[var(--g5-text0)]");
      expect(read.querySelector("svg")).not.toBeNull();
      const again = await requirement.findByRole("button", {
        name: "Request again",
      });
      expect(again).toHaveClass("underline-offset-2");
      expect(again).not.toHaveClass("bg-[var(--g5-text0)]");
    });

    it("keeps each request note with its request, opening only the viewer's own and keeping manual choices", async () => {
      const shared = "Please check cancellation.\nNo UI change.";
      const state = stateWith({
        state: "pending",
        callerCanDecide: true,
        agentEscalation: null,
        requests: [
          request({
            requestId: "alex-request",
            actorUserId: "user-alex",
            actor: { userId: "user-alex", displayName: "Alex Chen" },
            requestedBy: {
              userId: "user-jordan",
              displayName: "Jordan Rivera",
            },
            note: shared,
          }),
          request({
            requestId: "ben-request",
            actorUserId: "user-ben",
            actor: { userId: "user-ben", displayName: "Ben" },
            requestedBy: null,
            note: shared,
          }),
          request({
            requestId: "maya-request",
            actorUserId: "user-maya",
            actor: { userId: "user-maya", displayName: "Maya Chen" },
          }),
        ],
        decisionLog: [],
        approvalPath: null,
      });
      seedMockWorkspaceApprovals(WS_ID, state);
      const view = renderTab();
      const requirement = await ontologyRequirement();
      const notes = requirement.getAllByTestId("review-request-note");
      expect(notes).toHaveLength(2);
      const [alex, ben] = notes;
      expect(alex).toHaveAttribute("open");
      expect(alex).toHaveTextContent("Request note from Jordan Rivera");
      expect(
        within(alex).getByTestId("review-request-note-text").textContent,
      ).toBe(shared);
      expect(ben).not.toHaveAttribute("open");
      expect(ben).toHaveTextContent("Request note · Requester unavailable");

      await userEvent.click(within(alex).getByText(/^Request note from/));
      await userEvent.click(within(ben).getByText(/^Request note/));
      expect(alex).not.toHaveAttribute("open");
      expect(ben).toHaveAttribute("open");
      await view.queryClient.invalidateQueries();
      await waitFor(() =>
        expect(
          screen
            .getAllByTestId("review-request-note")
            .map((note) => (note as HTMLDetailsElement).open),
        ).toEqual([false, true]),
      );
    });

    it("leads with a link-free banner of approvals requested from the viewer", async () => {
      const state = forkState();
      state.outgoing!.requirements.push({
        ...state.outgoing!.requirements[0],
        definition: makeDefinition({ id: "arch", name: "Architecture" }),
        state: "approved",
      });
      seedMockWorkspaceApprovals(WS_ID, state);
      renderTab();
      const banner = await screen.findByTestId("personal-approvals-banner");
      expect(
        within(banner).getByRole("heading", {
          name: "2 approvals requested from you",
        }),
      ).toBeVisible();
      expect(banner).toHaveTextContent("Jordan Rivera requested your review.");
      expect(
        within(banner).getByTestId("personal-approvals-names"),
      ).toHaveTextContent(/^Architecture · Production merge sign-off$/);
      expect(within(banner).queryAllByRole("link")).toHaveLength(0);
      expect(within(banner).queryByRole("button")).toBeNull();
    });

    it("says under the heading when the open PR into the parent merges when ready", async () => {
      const state = forkState();
      state.outgoing!.requirements[0].callerCanDecide = false;
      seedMockWorkspaceApprovals(WS_ID, state);
      const target = state.outgoing!.targetWorkspaceId;
      const pull = (
        status: string,
        targetWorkspaceId: string,
        autoMergeRequest: object | null,
      ) => ({
        id: `pr-${status}-${targetWorkspaceId}`,
        status,
        targetWorkspaceId,
        createdAt: "2026-11-05T10:00:00Z",
        autoMergeRequest,
      });
      const on = {
        enabledBy: "user-ben",
        enabledAt: "2026-11-05T10:05:00Z",
        mergeMethod: "REBASE",
        reason: null,
        checkedAt: null,
      };
      let pulls = [pull("open", target, on)];
      server.use(
        http.get("/api/v1/workspaces/:id/pull-requests", () =>
          HttpResponse.json({ data: pulls }),
        ),
      );
      const view = renderTab();
      const note = await screen.findByTestId(
        "workspace-approvals-merge-when-ready",
      );
      expect(note).toHaveTextContent(
        `Merge when ready is on. G5 merges into ${state.outgoing!.targetWorkspaceName} once checks pass and approvals are met.`,
      );
      // Shown to every visitor, not only to requested approvers.
      expect(screen.queryByTestId("personal-approvals-banner")).toBeNull();

      // Off, into another workspace, or no longer open: no line.
      for (const next of [
        [pull("open", target, null)],
        [pull("open", "ws-elsewhere", on)],
        [pull("merged", target, on)],
      ]) {
        pulls = next;
        await view.queryClient.invalidateQueries({
          queryKey: ["workspace-pull-requests"],
        });
        await waitFor(() =>
          expect(
            screen.queryByTestId("workspace-approvals-merge-when-ready"),
          ).toBeNull(),
        );
        pulls = [pull("open", target, on)];
        await view.queryClient.invalidateQueries({
          queryKey: ["workspace-pull-requests"],
        });
        await screen.findByTestId("workspace-approvals-merge-when-ready");
      }
    });

    it("reports a failed refresh in the banner as text only", async () => {
      seedMockWorkspaceApprovals(WS_ID, forkState());
      const view = renderTab();
      const banner = await screen.findByTestId("personal-approvals-banner");
      server.use(
        http.get("/api/v1/workspaces/:workspaceId/approvals", () =>
          HttpResponse.json({ error: "Unavailable" }, { status: 503 }),
        ),
      );
      await view.queryClient.invalidateQueries({
        queryKey: ["workspace-approvals", WS_ID],
      });
      await waitFor(() =>
        expect(banner).toHaveAttribute("data-state", "unavailable"),
      );
      expect(banner).toHaveTextContent("Couldn’t load your approval requests.");
      expect(within(banner).queryAllByRole("link")).toHaveLength(0);
      expect(within(banner).queryByRole("button")).toBeNull();
      expect(
        screen.getByTestId("workspace-approvals-refresh-error"),
      ).toHaveTextContent("Could not refresh approvals");
    });

    it("omits the banner when nothing is requested from the viewer", async () => {
      const state = forkState();
      state.outgoing!.requirements[0].callerCanDecide = false;
      seedMockWorkspaceApprovals(WS_ID, state);
      renderTab();
      await screen.findByTestId("workspace-approvals-outgoing");
      expect(screen.queryByTestId("personal-approvals-banner")).toBeNull();
    });
  });
});
