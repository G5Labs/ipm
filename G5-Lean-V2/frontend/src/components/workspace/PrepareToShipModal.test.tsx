// Vitest coverage for the Prepare-to-Ship modal (prepare-to-ship-modal D5;
// scenario test_contract/workspace_pull_requests_phase1 I12): section order,
// approvals summary derivations, preview prefill + send-only-when-edited,
// create/update honesty copy, behind count, ported rebase options, and ship
// gating on PR readiness only — never on approvals.
import { useState } from "react";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { server } from "../../test/msw/server";
import type {
  AgentLaunchPreferencesV1,
  MergeOverridePreview,
  OrgApprovalDefinitionDTO,
  ResolvedAgentExecutionV1,
  SyncAgentConfig,
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingRequirementDTO,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { PrepareToShipModal } from "./PrepareToShipModal";
import type { PrepareToShipModalProps } from "./PrepareToShipModal";

const AGENT_CONFIG: SyncAgentConfig = {
  agentProvider: "claude_code",
  permissionMode: "bypassPermissions",
  permissionLabel: "Bypass",
  model: undefined,
  modelLabel: "Opus 4.8 [1M]",
};

function resolvedAgentLaunch(
  preferences: AgentLaunchPreferencesV1,
): ResolvedAgentExecutionV1 {
  return {
    schemaVersion: 1,
    resolutionId: "resolution-claude",
    configRevision: "config-1",
    catalogVersion: "catalog-1",
    provider: { value: "claude_code", provenance: "configured" },
    providerRoute: "anthropic_direct",
    model: { value: "claude-opus-4-8[1m]", provenance: "configured" },
    upstreamModel: "anthropic/claude-opus-4-8[1m]",
    permissionMode: { value: "bypassPermissions", provenance: "configured" },
    effort: {
      value:
        preferences.effort.source === "explicit"
          ? preferences.effort.value
          : "high",
      provenance: "configured",
    },
    authMode: "anthropic_api_key",
    egressMode: "direct_provider",
    credentialReady: true,
    requiresInteractiveAuth: false,
  };
}

function definitionFixture(
  overrides: Partial<OrgApprovalDefinitionDTO> = {},
): OrgApprovalDefinitionDTO {
  return {
    id: "def-1",
    latestVersion: 1,
    name: "Double signoff",
    description: null,
    humans: [],
    agentType: null,
    agentPrompt: null,
    invalidatesPrevious: false,
    createdBy: { userId: "u-dvd", displayName: "dvd-g5" },
    createdAt: "2026-08-12T00:00:00Z",
    firstCreatedBy: { userId: "u-dvd", displayName: "dvd-g5" },
    firstCreatedAt: "2026-08-12T00:00:00Z",
    ...overrides,
  };
}

function approverFixture(
  userId: string,
  displayName: string,
  state: "approved" | "rejected" | "pending",
  decidedAt: string | null = null,
) {
  return {
    userId,
    displayName,
    email: `${userId}@g5labs.dev`,
    stillMember: true,
    state,
    comment: null,
    decidedAt,
  };
}

/**
 * A workspace approvals state with the verdict the server projects from its
 * IE entries (g5.governance.approval.requirements); `withAdmission: false`
 * models a summary the server has not evaluated.
 */
function approvalsStateFixture(
  requirements: WorkspaceOutgoingRequirementDTO[],
  withAdmission = true,
): WorkspaceApprovalsStateDTO {
  const blocking = requirements.filter((entry) => entry.state !== "approved");
  return {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "ws-target",
      targetWorkspaceName: "Base",
      requirements,
      mergeAdmission: withAdmission
        ? {
            admitted: blocking.length === 0,
            reason:
              blocking.length === 0
                ? null
                : "Approval(s) required before merging",
            requirements: requirements.map((entry) => ({
              definitionId: entry.definition.id,
              definitionName: entry.definition.name,
              definitionVersion: entry.definition.latestVersion,
              state: entry.state,
            })),
          }
        : undefined,
    },
  };
}

const PENDING_APPROVALS = approvalsStateFixture([
  {
    definition: definitionFixture(),
    state: "pending",
    approvers: [
      approverFixture("u-dvd", "dvd-g5", "approved", "2026-08-13T21:10:00Z"),
      approverFixture("u-kai", "kaihuiz-g5", "pending"),
    ],
    callerCanDecide: true,
    decisionLog: [],
  },
]);

const READY_APPROVALS = approvalsStateFixture([
  {
    definition: definitionFixture(),
    state: "approved",
    approvers: [
      approverFixture("u-dvd", "dvd-g5", "approved", "2026-08-13T21:10:00Z"),
    ],
    callerCanDecide: true,
    decisionLog: [],
  },
]);

function pullRequestFixture(
  overrides: Partial<WorkspacePullRequestSummary> = {},
): WorkspacePullRequestSummary {
  return {
    id: "pr-1",
    sourceWorkspaceId: "ws-source",
    sourceWorkspaceName: "Fork",
    targetWorkspaceId: "ws-target",
    targetWorkspaceName: "Base",
    status: "open",
    localMergeStatus: "ready",
    providerStatus: "local_only",
    title: "feat: derived title",
    descriptionOverride: null,
    sourceHeads: [],
    targetHeads: [],
    preflight: {} as never,
    lastPreflightFingerprint: "fingerprint",
    blockingMessage: null,
    createdAt: "2026-08-14T00:00:00Z",
    updatedAt: "2026-08-14T00:00:00Z",
    mergedAt: null,
    ...overrides,
  };
}

const PREVIEW_BODY = "- `feat: derived title`\n- `fix: follow-up`";

function usePreviewHandler() {
  server.use(
    http.post("/api/v1/workspaces/ws-source/pull-requests/preview", () =>
      HttpResponse.json({
        data: {
          title: "feat: derived title",
          body: PREVIEW_BODY,
          derived: true,
        },
      }),
    ),
  );
}

/**
 * The dialog reads the shared approvals response itself
 * (g5.workbench.ship-approvals); serve the same state the shell passes.
 */
function serveApprovals(
  state: WorkspaceApprovalsStateDTO | null | undefined,
  workspaceId = "ws-source",
) {
  server.use(
    http.get(`/api/v1/workspaces/${workspaceId}/approvals`, () =>
      HttpResponse.json({
        data: state ?? {
          requiredInto: [],
          attachmentHistory: [],
          outgoing: null,
        },
      }),
    ),
  );
}

function renderModal(
  overrides: Partial<PrepareToShipModalProps> = {},
  injectedQueryClient?: QueryClient,
) {
  const queryClient =
    injectedQueryClient ??
    new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
  const props: PrepareToShipModalProps = {
    workspaceId: "ws-source",
    orgId: "org-1",
    sourceWorkspaceName: "Fork",
    targetWorkspaceId: "ws-target",
    targetWorkspaceName: "Base",
    repositoryComparison: {
      repositories: [
        {
          repoId: "repo-code",
          repoName: "g5-code",
          behindBase: 1,
          divergence: 0,
        },
        {
          repoId: "repo-docs",
          repoName: "g5-docs",
          behindBase: 1,
          divergence: 1,
        },
      ],
      hasBehind: true,
      hasUnknownBehind: false,
      allBehindKnownZero: false,
    },
    shipApprovals: PENDING_APPROVALS,
    existingPullRequest: null,
    agentConfig: AGENT_CONFIG,
    resolveAgentLaunch: vi.fn(async (preferences: AgentLaunchPreferencesV1) =>
      resolvedAgentLaunch(preferences),
    ),
    onStartRebase: vi.fn(),
    onReviewApprovals: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
  serveApprovals(props.shipApprovals);
  return {
    props,
    queryClient,
    ...render(
      <QueryClientProvider client={queryClient}>
        <PrepareToShipModal {...props} />
      </QueryClientProvider>,
    ),
  };
}

describe("PrepareToShipModal", () => {
  it("disables Prepare and Update codebase once the source is a review copy or unowned, keeping the description and sending nothing (g5.workspace.ui.reader-publication)", async () => {
    usePreviewHandler();
    let posted = false;
    server.use(
      http.post("/api/v1/workspaces/ws-source/pull-requests", () => {
        posted = true;
        return HttpResponse.json({ data: {} }, { status: 500 });
      }),
    );
    const gate = (owner: boolean) => ({
      kind: owner ? ("stopped" as const) : ("reader" as const),
      canWrite: owner,
      live: false,
      canStart: owner,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner,
      workspaceNoun: "the workspace",
    });
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    const onStartRebase = vi.fn();
    function Host() {
      const [owner, setOwner] = useState(true);
      return (
        <WorkspaceLiveGateProvider value={gate(owner)}>
          <button
            type="button"
            data-testid="lose-ownership"
            onClick={() => setOwner(false)}
          >
            transfer
          </button>
          <PrepareToShipModal
            workspaceId="ws-source"
            orgId="org-1"
            sourceWorkspaceName="Fork"
            targetWorkspaceId="ws-target"
            targetWorkspaceName="Base"
            repositoryComparison={{
              repositories: [
                {
                  repoId: "repo-code",
                  repoName: "g5-code",
                  behindBase: 1,
                  divergence: 0,
                },
              ],
              hasBehind: true,
              hasUnknownBehind: false,
              allBehindKnownZero: false,
            }}
            shipApprovals={PENDING_APPROVALS}
            existingPullRequest={null}
            agentConfig={AGENT_CONFIG}
            resolveAgentLaunch={vi.fn(
              async (preferences: AgentLaunchPreferencesV1) =>
                resolvedAgentLaunch(preferences),
            )}
            onStartRebase={onStartRebase}
            onReviewApprovals={vi.fn()}
            onClose={vi.fn()}
          />
        </WorkspaceLiveGateProvider>
      );
    }
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <Host />
      </QueryClientProvider>,
    );
    const create = screen.getByTestId("prepare-to-ship-create");
    const update = screen.getByTestId("prepare-to-ship-update-codebase");
    // A stopped owner keeps both: they are durable actions.
    expect(create).toBeEnabled();
    await waitFor(() => expect(update).toBeEnabled());
    expect(screen.queryByTestId("prepare-to-ship-blocked")).toBeNull();

    await user.click(screen.getByTestId("lose-ownership"));

    await waitFor(() => expect(create).toBeDisabled());
    expect(update).toBeDisabled();
    expect(screen.getByTestId("prepare-to-ship-blocked")).toHaveTextContent(
      "Only the owner can publish from this workspace.",
    );
    fireEvent.click(create);
    fireEvent.click(update);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(posted).toBe(false);
    expect(onStartRebase).not.toHaveBeenCalled();
  });

  it("uses the new PR identity until a stale predecessor query catches up", async () => {
    usePreviewHandler();
    const created = pullRequestFixture({
      id: "new-pr",
      blockingMessage: "New PR pending",
    });
    server.use(
      http.post("/api/v1/workspaces/ws-source/pull-requests", () =>
        HttpResponse.json({
          data: {
            pullRequest: created,
            alreadyExisted: false,
            dirtyDrafts: [],
          },
        }),
      ),
    );
    renderModal({
      existingPullRequest: pullRequestFixture({
        blockingMessage: "Old PR failure",
      }),
    });
    await userEvent.setup().click(screen.getByTestId("prepare-to-ship-create"));
    await waitFor(() =>
      expect(screen.getByTestId("prepare-to-ship-error")).toHaveTextContent(
        "New PR pending",
      ),
    );
  });

  it("follows live recovery and later failure after creating in the open modal", async () => {
    usePreviewHandler();
    const pending = pullRequestFixture({
      blockingMessage: "Initial failure",
    });
    server.use(
      http.post("/api/v1/workspaces/ws-source/pull-requests", () =>
        HttpResponse.json({
          data: {
            pullRequest: pending,
            alreadyExisted: false,
            dirtyDrafts: [],
          },
        }),
      ),
    );
    const { props, queryClient, rerender } = renderModal();
    await userEvent.setup().click(screen.getByTestId("prepare-to-ship-create"));
    await waitFor(() =>
      expect(screen.getByTestId("prepare-to-ship-error")).toHaveTextContent(
        "Initial failure",
      ),
    );
    const updateLive = (pullRequest: WorkspacePullRequestSummary) =>
      rerender(
        <QueryClientProvider client={queryClient}>
          <PrepareToShipModal {...props} existingPullRequest={pullRequest} />
        </QueryClientProvider>,
      );
    updateLive(pullRequestFixture());
    expect(
      screen.queryByTestId("prepare-to-ship-error"),
    ).not.toBeInTheDocument();
    updateLive({ ...pending, blockingMessage: "Later push failed" });
    expect(screen.getByTestId("prepare-to-ship-error")).toHaveTextContent(
      "Later push failed",
    );
  });

  it("shows pending automatic refresh separately from local readiness", () => {
    usePreviewHandler();
    renderModal({
      existingPullRequest: {
        id: "pr-1",
        status: "open",
        localMergeStatus: "ready",
        providerStatus: "open",
        title: "Test PR",
        sourceHeads: [],
        targetHeads: [],
        providerRepos: [],
        blockingMessage: "Automatic refresh failed; choose Update PR to retry.",
      } as unknown as WorkspacePullRequestSummary,
    });
    expect(screen.getByTestId("prepare-to-ship-error")).toHaveTextContent(
      "Update PR",
    );
  });

  it("renders the four sections in order with no approve-changes affordance", () => {
    usePreviewHandler();
    renderModal();
    const sections = [
      screen.getByTestId("prepare-to-ship-approvals"),
      screen.getByTestId("prepare-to-ship-prepare"),
      screen.getByTestId("prepare-to-ship-update"),
      screen.getByTestId("prepare-to-ship-ship"),
    ];
    for (let index = 1; index < sections.length; index += 1) {
      expect(
        sections[index - 1].compareDocumentPosition(sections[index]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
    expect(screen.queryByText(/approve changes/i)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /^approve$/i }),
    ).not.toBeInTheDocument();
  });

  it("reads approval readiness without creating human requests or agent jobs", async () => {
    let requestPosts = 0;
    server.use(
      http.post("*/api/v1/workspaces/:workspaceId/approvals/requests", () => {
        requestPosts += 1;
        return HttpResponse.json({ data: { requests: [] } }, { status: 201 });
      }),
      http.post("*/api/v1/workspaces/:workspaceId/approvals/reviews", () => {
        requestPosts += 1;
        return HttpResponse.json(
          {
            data: { requestId: "unexpected", status: "pending", reused: false },
          },
          { status: 201 },
        );
      }),
    );
    usePreviewHandler();
    renderModal();
    await screen.findByTestId("prepare-to-ship-approvals");
    // Negative proof: `waitFor(() => expect(0))` resolves on its first tick
    // and can never fail, so flush queued network macrotasks explicitly and
    // assert directly — a late POST now fails the test.
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(requestPosts).toBe(0);
  });

  it("applies the D6 wording pass — target-named sections, PR-consequence labels, warn ship tone, no enforcement footnote", () => {
    usePreviewHandler();
    renderModal();
    // Approvals footnote removed; the advisory posture itself is unchanged.
    expect(
      screen.queryByText(/Decisions are recorded, not yet enforced/i),
    ).not.toBeInTheDocument();
    // Create button states its PR-opening consequence.
    expect(screen.getByTestId("prepare-to-ship-create")).toHaveTextContent(
      "Prepare to ship - open Pull Request (PR)",
    );
    // Section 3 heading and action button name the target workspace, and the
    // description states the one-sided rebase in workspace names.
    const update = screen.getByTestId("prepare-to-ship-update");
    expect(update).toHaveTextContent("Get the latest changes from Base");
    expect(
      screen.getByTestId("prepare-to-ship-update-codebase"),
    ).toHaveTextContent("Get the latest changes from Base");
    expect(update).toHaveTextContent(
      "Get the latest changes from Base into this workspace. Only Fork will be changed. An agent resolves conflicts in its running workspace; review the result before shipping.",
    );
    expect(update).not.toHaveTextContent(/Rebases this workspace onto/i);
    // Ship keeps its label but renders in the warn treatment, never the ok
    // (green) tint (spec D6).
    const ship = screen.getByTestId("prepare-to-ship-ship-button");
    expect(ship).toHaveTextContent("Ship to parent");
    expect(ship.className).toContain("--g5-warn");
    expect(ship.className).not.toContain("--g5-ok");
    // Merge description keeps the dynamic target name.
    expect(screen.getByTestId("prepare-to-ship-ship")).toHaveTextContent(
      "Merges the pull request into Base. Not a deployment.",
    );
  });

  it("counts approval requirements, not approvers, with one icon per requirement and a message naming the unsatisfied ones (g5.workbench.ship-approvals)", () => {
    usePreviewHandler();
    // One pending requirement with one of its two approvers approved counts
    // as one pending requirement.
    const first = renderModal();
    expect(
      screen.getByTestId("prepare-to-ship-approvals-counts"),
    ).toHaveTextContent("0 approved, 0 rejected, 1 pending");
    expect(
      screen.getByTestId("prepare-to-ship-approvals-message"),
    ).toHaveTextContent(/^Double signoff pending$/);
    first.unmount();

    usePreviewHandler();
    const second = renderModal({ shipApprovals: READY_APPROVALS });
    expect(
      screen.getByTestId("prepare-to-ship-approvals-counts"),
    ).toHaveTextContent("1 approved, 0 rejected, 0 pending");
    expect(
      screen.getByTestId("prepare-to-ship-approvals-message"),
    ).toHaveTextContent("All approval requirements are approved");
    expect(
      screen.getByRole("img", { name: "Double signoff: Approved" }),
    ).toHaveTextContent("✓");
    second.unmount();

    // Four requirements in the response's (alphabetical) order, including an
    // any-of whose agent rejected and a human approval superseded: the
    // server's state is approved, so it counts as approved once.
    usePreviewHandler();
    const requirement = (
      id: string,
      name: string,
      state: "approved" | "rejected" | "pending",
    ): WorkspaceOutgoingRequirementDTO => ({
      definition: definitionFixture({ id, name }),
      state,
      approvers: [
        approverFixture(`u-${id}`, `Reviewer ${id}`, "rejected"),
        approverFixture(`u2-${id}`, `Reviewer 2 ${id}`, "approved"),
      ],
      callerCanDecide: false,
      decisionLog: [],
    });
    const mixed = renderModal({
      shipApprovals: approvalsStateFixture([
        requirement("a", "Architecture", "approved"),
        requirement("d", "Documentation", "rejected"),
        requirement("s", "Security review", "pending"),
        requirement("u", "UI changes", "approved"),
      ]),
    });
    expect(
      screen.getByTestId("prepare-to-ship-approvals-counts"),
    ).toHaveTextContent("2 approved, 1 rejected, 1 pending");
    const icons = screen.getAllByTestId("prepare-to-ship-approval-icon");
    expect(icons.map((icon) => icon.getAttribute("aria-label"))).toEqual([
      "Architecture: Approved",
      "Documentation: Rejected",
      "Security review: Pending",
      "UI changes: Approved",
    ]);
    expect(icons.map((icon) => icon.textContent)).toEqual(["✓", "×", "○", "✓"]);
    for (const icon of icons)
      expect(icon.className).toContain("text-[var(--g5-text0)]");
    expect(
      screen.getByTestId("prepare-to-ship-approvals-message"),
    ).toHaveTextContent(/^Documentation rejected; Security review pending$/);
    // View approvals stays in place.
    expect(
      within(screen.getByTestId("prepare-to-ship-approvals")).getByTestId(
        "prepare-to-ship-review-approvals",
      ),
    ).toHaveTextContent("View approvals");
    // ④ names no requirement: the one generic approvals sentence.
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      /^Approval\(s\) required before merging$/,
    );
    mixed.unmount();

    usePreviewHandler();
    renderModal({ shipApprovals: null });
    expect(screen.getByText(/no approvals are required/i)).toBeInTheDocument();
  });

  it("reads the shared approvals response itself while open, ahead of the shell's copy (g5.workbench.ship-approvals)", async () => {
    usePreviewHandler();
    let reads = 0;
    renderModal({ shipApprovals: PENDING_APPROVALS });
    server.use(
      http.get("/api/v1/workspaces/ws-source/approvals", () => {
        reads += 1;
        return HttpResponse.json({ data: READY_APPROVALS });
      }),
    );
    await waitFor(
      () =>
        expect(
          screen.getByTestId("prepare-to-ship-approvals-counts"),
        ).toHaveTextContent("1 approved, 0 rejected, 0 pending"),
      { timeout: 3_000 },
    );
    expect(reads).toBeGreaterThan(0);
    expect(
      screen.getByTestId("prepare-to-ship-approvals-message"),
    ).toHaveTextContent("All approval requirements are approved");
  });

  it("prefills the collapsed description from the preview and sends description only when edited", async () => {
    usePreviewHandler();
    const createBodies: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/ws-source/pull-requests",
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          createBodies.push(body);
          return HttpResponse.json({
            data: {
              pullRequest: pullRequestFixture(
                typeof body.description === "string"
                  ? { descriptionOverride: body.description }
                  : {},
              ),
              alreadyExisted: createBodies.length > 1,
              dirtyDrafts: [],
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderModal();

    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-description-toggle"),
      ).toHaveTextContent(/pre-populated from your commits/i),
    );
    // Collapsed by default; expanding reveals the derived body.
    expect(
      screen.queryByTestId("prepare-to-ship-description"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("prepare-to-ship-description-toggle"));
    expect(screen.getByTestId("prepare-to-ship-description")).toHaveValue(
      PREVIEW_BODY,
    );

    // Untouched create omits the description field entirely.
    await user.click(screen.getByTestId("prepare-to-ship-create"));
    await waitFor(() => expect(createBodies).toHaveLength(1));
    expect(createBodies[0]).toEqual({ targetWorkspaceId: "ws-target" });
    expect(screen.getByTestId("prepare-to-ship-result")).toHaveTextContent(
      "PR created.",
    );
    expect(screen.getByTestId("prepare-to-ship-create")).toHaveTextContent(
      "Update Pull Request (PR)",
    );

    // Edited description rides the next create/update call.
    const textarea = screen.getByTestId("prepare-to-ship-description");
    await user.clear(textarea);
    await user.type(textarea, "My hand-written summary");
    await user.click(screen.getByTestId("prepare-to-ship-create"));
    await waitFor(() => expect(createBodies).toHaveLength(2));
    expect(createBodies[1]).toEqual({
      targetWorkspaceId: "ws-target",
      description: "My hand-written summary",
    });
  });

  it("attributes the prefill to the Project Docs pr_body when the preview source is projectDoc", async () => {
    // Backend spec D16: honest body-source attribution on the disclosure.
    server.use(
      http.post("/api/v1/workspaces/ws-source/pull-requests/preview", () =>
        HttpResponse.json({
          data: {
            title: "feat: derived title",
            body: "Authored body from the workspace PR-body Project Doc",
            derived: true,
            source: "projectDoc",
          },
        }),
      ),
    );
    renderModal();

    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-description-toggle"),
      ).toHaveTextContent(/pre-populated from pr_body in Project Docs/i),
    );
  });

  it.each([
    [
      "failed",
      "GitHub returned 422 Unprocessable Entity: Validation Failed; not all refs are readable",
      false,
    ],
    [
      "action_required",
      "GitHub returned 422 Unprocessable Entity: Validation Failed; not all refs are readable",
      false,
    ],
    [
      "action_required",
      "github_user_connection_required: Connect or reconnect GitHub, then retry.",
      true,
    ],
  ] as const)(
    "shows the reason for %s GitHub PR creation (%s) and clears it after retry",
    async (status, error, connectionRequired) => {
      usePreviewHandler();
      let attempts = 0;
      server.use(
        http.post("/api/v1/workspaces/ws-source/pull-requests", () => {
          const recovered = ++attempts > 1;
          return HttpResponse.json({
            data: {
              pullRequest: pullRequestFixture({
                providerStatus: recovered ? "open" : status,
                providerRepos: [
                  {
                    id: "provider-code",
                    repoId: "repo-code",
                    provider: "github",
                    providerRepoFullName: "acme/code",
                    sourceRemoteBranch: "g5/fork",
                    targetRemoteBranch: "main",
                    providerPrNumber: recovered ? 77 : null,
                    providerPrUrl: recovered
                      ? "https://github.com/acme/code/pull/77"
                      : null,
                    providerStatus: recovered ? "open" : status,
                    sourceHeadSha: null,
                    targetHeadSha: null,
                    providerHeadSha: null,
                    providerBaseSha: null,
                    lastError: recovered ? null : error,
                    lastSyncedAt: null,
                    updatedAt: "2026-08-14T00:00:00Z",
                  },
                ],
              }),
              alreadyExisted: recovered,
              dirtyDrafts: [],
            },
          });
        }),
      );
      const user = userEvent.setup();
      renderModal();
      await user.click(screen.getByTestId("prepare-to-ship-create"));
      await waitFor(() =>
        expect(screen.getByTestId("prepare-to-ship-result")).toHaveTextContent(
          "Local PR created — GitHub PR not created.",
        ),
      );
      expect(
        screen.getByTestId("prepare-to-ship-provider-errors"),
      ).toHaveTextContent(
        `acme/code: ${error.replace("github_user_connection_required: ", "")}`,
      );
      expect(
        screen.getByTestId("prepare-to-ship-provider-errors"),
      ).toHaveTextContent("Saved commits are kept");
      const connectionLink = screen.queryByRole("link", {
        name: "Manage GitHub connection in Account settings",
      });
      if (connectionRequired) {
        expect(connectionLink).toHaveAttribute(
          "href",
          `/account/github?returnTo=${encodeURIComponent(window.location.pathname + window.location.search)}`,
        );
      } else {
        expect(connectionLink).not.toBeInTheDocument();
      }
      expect(
        within(screen.getByTestId("prepare-to-ship-prepare")).getByText("2"),
      ).toBeInTheDocument();
      await user.click(screen.getByTestId("prepare-to-ship-create"));
      await waitFor(() =>
        expect(screen.getByTestId("prepare-to-ship-result")).toHaveTextContent(
          "PR updated.",
        ),
      );
      expect(
        screen.queryByTestId("prepare-to-ship-provider-errors"),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("link", { name: "acme/code#77" }),
      ).toHaveAttribute("href", "https://github.com/acme/code/pull/77");
    },
  );

  it("visually separates semibold repository names while showing behind-only status", async () => {
    usePreviewHandler();
    const user = userEvent.setup();
    const { props } = renderModal();
    const comparison = screen.getByTestId("prepare-to-ship-behind-count");
    expect(comparison).toHaveTextContent(
      "g5-code: 1 behind │ g5-docs: 1 behind",
    );
    expect(comparison).not.toHaveTextContent("ahead");
    expect(within(comparison).getByText("g5-code:")).toHaveClass(
      "font-semibold",
    );
    expect(within(comparison).getByText("g5-docs:")).toHaveClass(
      "font-semibold",
    );

    // The Update button waits for the launch resolution to confirm.
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-update-codebase"),
      ).toBeEnabled(),
    );
    await user.click(
      screen.getByTestId("prepare-to-ship-rebase-options-toggle"),
    );
    await user.click(screen.getByTestId("prepare-to-ship-update-codebase"));
    expect(props.onStartRebase).toHaveBeenCalledWith(
      expect.objectContaining({ agentProvider: "claude_code" }),
      expect.objectContaining({
        confirmedResolutionId: "resolution-claude",
      }),
    );
  });

  it("shows up-to-date and marks the step done only when every branch reports a known zero", () => {
    usePreviewHandler();
    renderModal({
      repositoryComparison: {
        repositories: [
          {
            repoId: "repo-any",
            repoName: "any-repository",
            behindBase: 0,
            divergence: 0,
          },
        ],
        hasBehind: false,
        hasUnknownBehind: false,
        allBehindKnownZero: true,
      },
    });
    const badge = screen.getByTestId("prepare-to-ship-behind-count");
    expect(badge).toHaveTextContent("Up to date with Base");
    expect(badge).toHaveAttribute("data-state", "upToDate");
    // Step 3's heading renders "✓" when done.
    expect(
      within(screen.getByTestId("prepare-to-ship-update")).getByText("✓"),
    ).toBeInTheDocument();
  });

  it("hides ahead-only history and keeps the Update step done", () => {
    usePreviewHandler();
    renderModal({
      repositoryComparison: {
        repositories: [
          {
            repoId: "repo-ahead",
            repoName: "release-manifests",
            behindBase: 0,
            divergence: 3,
          },
        ],
        hasBehind: false,
        hasUnknownBehind: false,
        allBehindKnownZero: true,
      },
    });
    const badge = screen.getByTestId("prepare-to-ship-behind-count");
    expect(badge).toHaveTextContent("Up to date with Base");
    expect(badge).not.toHaveTextContent("ahead");
    expect(badge).not.toHaveTextContent("release-manifests");
    expect(badge).toHaveAttribute("data-state", "upToDate");
    expect(
      within(screen.getByTestId("prepare-to-ship-update")).getByText("✓"),
    ).toBeInTheDocument();
  });

  it("names the repository and never renders green while its behind count is unknown", () => {
    usePreviewHandler();
    renderModal({
      repositoryComparison: {
        repositories: [
          {
            repoId: "repo-private",
            repoName: "private-service",
            behindBase: null,
            divergence: null,
          },
        ],
        hasBehind: false,
        hasUnknownBehind: true,
        allBehindKnownZero: false,
      },
    });
    const badge = screen.getByTestId("prepare-to-ship-behind-count");
    expect(badge).toHaveTextContent("private-service: comparison unknown");
    expect(badge).toHaveAttribute("data-state", "unknown");
    // The step must NOT be done: the numbered heading (not "✓") renders.
    const update = screen.getByTestId("prepare-to-ship-update");
    expect(within(update).queryByText("✓")).not.toBeInTheDocument();
    expect(within(update).getByText("3")).toBeInTheDocument();
  });

  it("keeps known commits actionable alongside a named unknown repository", () => {
    usePreviewHandler();
    renderModal({
      repositoryComparison: {
        repositories: [
          {
            repoId: "repo-known",
            repoName: "orders",
            behindBase: 2,
            divergence: 0,
          },
          {
            repoId: "repo-unknown",
            repoName: "inventory",
            behindBase: null,
            divergence: null,
          },
        ],
        hasBehind: true,
        hasUnknownBehind: true,
        allBehindKnownZero: false,
      },
    });
    const badge = screen.getByTestId("prepare-to-ship-behind-count");
    expect(badge).toHaveTextContent(
      "orders: 2 behind │ inventory: comparison unknown",
    );
    expect(badge).toHaveAttribute("data-state", "behind");
    expect(
      within(screen.getByTestId("prepare-to-ship-update")).queryByText("✓"),
    ).not.toBeInTheDocument();
  });

  it("renders no badge and no done state when the detail carries no summary", () => {
    usePreviewHandler();
    renderModal({ repositoryComparison: null });
    expect(
      screen.queryByTestId("prepare-to-ship-behind-count"),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("prepare-to-ship-update")).queryByText("✓"),
    ).not.toBeInTheDocument();
  });

  it("invalidates the workspace-detail query exactly once on open (D9 freshness)", async () => {
    usePreviewHandler();
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    const spy = vi.spyOn(queryClient, "invalidateQueries");
    const detailCalls = () =>
      spy.mock.calls.filter(
        (call) =>
          JSON.stringify(call[0]?.queryKey) === '["workspace","ws-source"]',
      ).length;

    const { props, rerender } = renderModal({}, queryClient);
    await waitFor(() => expect(detailCalls()).toBe(1));

    // A re-render (e.g. the refreshed detail landing) must not re-fire the
    // one-shot invalidation — that would loop the refetch.
    rerender(
      <QueryClientProvider client={queryClient}>
        <PrepareToShipModal
          {...props}
          repositoryComparison={{
            repositories: [
              {
                repoId: "repo-code",
                repoName: "g5-code",
                behindBase: 1,
                divergence: 0,
              },
            ],
            hasBehind: true,
            hasUnknownBehind: false,
            allBehindKnownZero: false,
          }}
        />
      </QueryClientProvider>,
    );
    expect(detailCalls()).toBe(1);
  });

  it("disables ship without a PR, when the PR is not ready, when the server refuses admission, and while the verdict is unknown", async () => {
    usePreviewHandler();
    const noPr = renderModal({ shipApprovals: READY_APPROVALS });
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      /create the pr first/i,
    );
    noPr.unmount();

    // The admission refusal is named before the missing PR: it is the
    // reason to act on first.
    usePreviewHandler();
    const noPrPending = renderModal();
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      /^Approval\(s\) required before merging$/,
    );
    noPrPending.unmount();

    usePreviewHandler();
    const conflicted = renderModal({
      existingPullRequest: pullRequestFixture({
        localMergeStatus: "conflict",
        blockingMessage: "Git conflicts in 2 files",
        descriptionOverride: "stored override",
      }),
      shipApprovals: READY_APPROVALS,
    });
    // A blocker disables Ship to parent and makes Ship with agent the
    // primary control (g5.workbench.ship-to-parent).
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    expect(
      screen.getByTestId("prepare-to-ship-ship-with-agent"),
    ).toHaveAttribute("data-emphasis", "primary");
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "Git conflicts in 2 files",
    );
    conflicted.unmount();

    // The server's merge admission gates shipping
    // (g5.governance.approval.requirements): a refused requirement disables
    // the button with the one generic sentence, never the requirement names
    // an older server put in its reason (g5.workbench.ship-to-parent).
    usePreviewHandler();
    const refusedByPr = renderModal({
      existingPullRequest: pullRequestFixture({
        mergeAdmission: {
          admitted: false,
          reason: "Approval required before merging: Ship (pending)",
          requirements: [],
        },
      }),
      shipApprovals: READY_APPROVALS,
    });
    expect(
      screen.getByTestId("prepare-to-ship-ship-with-agent"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "Approval(s) required before merging",
    );
    expect(
      screen.getByTestId("prepare-to-ship-ship-note"),
    ).not.toHaveTextContent("Ship (pending)");
    refusedByPr.unmount();

    // A non-approval admission refusal keeps the server's own reason.
    usePreviewHandler();
    const wrongTarget = renderModal({
      existingPullRequest: pullRequestFixture({
        mergeAdmission: {
          admitted: false,
          reason: "Change the merge target first.",
          requirements: [],
        },
      }),
      shipApprovals: READY_APPROVALS,
    });
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "Change the merge target first.",
    );
    wrongTarget.unmount();

    usePreviewHandler();
    const refusedByState = renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: {
        ...PENDING_APPROVALS,
        outgoing: {
          ...PENDING_APPROVALS.outgoing!,
          mergeAdmission: {
            admitted: false,
            reason: "Approval required before merging",
            requirements: [],
          },
        },
      },
    });
    expect(
      screen.getByTestId("prepare-to-ship-ship-with-agent"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("prepare-to-ship-approvals-counts"),
    ).toHaveTextContent("0 approved, 0 rejected, 1 pending");
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "Approval(s) required before merging",
    );
    refusedByState.unmount();

    // Without a server verdict the readiness is unknown: nothing is
    // recomputed from the roster (ea7382c5: pending roster entries never
    // gate by themselves) and ship waits for the next refresh.
    usePreviewHandler();
    const unknown = renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: approvalsStateFixture(
        PENDING_APPROVALS.outgoing!.requirements,
        false,
      ),
    });
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "not evaluated yet",
    );
    unknown.unmount();

    // The server's admission alone enables shipping: the pending roster of
    // an admitted cut never gates (ea7382c5).
    usePreviewHandler();
    server.use(
      http.post("/api/v1/workspaces/ws-target/pull-requests/pr-1/merge", () =>
        HttpResponse.json({
          data: {
            pullRequest: pullRequestFixture({
              status: "merged",
              localMergeStatus: "merged",
              mergedAt: "2026-08-14T01:00:00Z",
            }),
            run: {},
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: {
        ...PENDING_APPROVALS,
        outgoing: {
          ...PENDING_APPROVALS.outgoing!,
          mergeAdmission: { admitted: true, reason: null, requirements: [] },
        },
      },
    });
    const shipButton = screen.getByTestId("prepare-to-ship-ship-button");
    expect(shipButton).toBeEnabled();
    await user.click(shipButton);
    await waitFor(() =>
      expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
        "Merged into Base.",
      ),
    );
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toHaveTextContent(
      "Shipped",
    );
  });

  it("switches merge when ready on and off without an agent while Ship waits (g5.workbench.ship-to-parent)", async () => {
    usePreviewHandler();
    const seen: string[] = [];
    const on = pullRequestFixture({
      autoMergeRequest: {
        enabledBy: "someone-else",
        enabledAt: "2026-08-14T00:30:00Z",
        mergeMethod: "REBASE",
        reason: "Waiting for external checks to finish.",
        checkedAt: "2026-08-14T00:31:00Z",
      },
    });
    server.use(
      http.post(
        "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
        async ({ request }) => {
          seen.push(`on ${JSON.stringify(await request.json())}`);
          return HttpResponse.json({ data: { pullRequest: on } });
        },
      ),
      http.delete(
        "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
        () => {
          seen.push("off");
          return HttpResponse.json({
            data: { pullRequest: pullRequestFixture() },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderModal({ existingPullRequest: pullRequestFixture() });
    const toggle = screen.getByTestId("prepare-to-ship-auto-merge-switch");
    expect(toggle).not.toBeChecked();
    await user.click(toggle);
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-auto-merge-status"),
      ).toHaveTextContent("Waiting for external checks to finish."),
    );
    expect(
      screen.getByTestId("prepare-to-ship-auto-merge-switch"),
    ).toBeChecked();
    await user.click(screen.getByTestId("prepare-to-ship-auto-merge-switch"));
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-auto-merge-switch"),
      ).not.toBeChecked(),
    );
    // The switch never starts an agent.
    expect(seen).toEqual(["on {}", "off"]);
  });

  it.each([false, true])(
    "saves the stop choice independently before shipping with agent=%s",
    async (withAgent) => {
      usePreviewHandler();
      const requests: unknown[] = [];
      let saved = pullRequestFixture();
      server.use(
        http.patch(
          "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
          async ({ request }) => {
            const input = (await request.json()) as {
              stopVmAfterMerge: boolean;
            };
            requests.push(input);
            saved = { ...saved, ...input };
            return HttpResponse.json({ data: { pullRequest: saved } });
          },
        ),
        http.post(
          "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
          async ({ request }) => {
            requests.push(await request.json());
            saved = {
              ...saved,
              autoMergeRequest: {
                enabledBy: "u-dvd",
                enabledAt: "2026-11-07T00:00:00Z",
                mergeMethod: "REBASE",
                reason: null,
                checkedAt: null,
              },
            };
            return HttpResponse.json({ data: { pullRequest: saved } });
          },
        ),
      );
      const user = userEvent.setup();
      renderModal({ existingPullRequest: pullRequestFixture() });
      const stop = screen.getByRole("switch", { name: "Stop VM after merge" });
      expect(stop).not.toBeChecked();
      expect(stop).toHaveAccessibleDescription(
        "Applies to all merge options below.",
      );
      await user.click(stop);
      await waitFor(() => expect(stop).toBeChecked());
      expect(requests).toEqual([{ stopVmAfterMerge: true }]);
      expect(
        screen.getByRole("switch", { name: "Merge when ready" }),
      ).not.toBeChecked();
      const ship = withAgent
        ? screen.getByTestId("prepare-to-ship-ship-with-agent")
        : screen.getByRole("switch", { name: "Merge when ready" });
      await waitFor(() => expect(ship).toBeEnabled());
      await user.click(ship);
      await waitFor(() =>
        expect(
          screen.getByRole("switch", { name: "Merge when ready" }),
        ).toBeChecked(),
      );
      expect(stop).toBeChecked();
      expect(requests).toHaveLength(2);
      expect(Boolean((requests[1] as { agent?: unknown }).agent)).toBe(
        withAgent,
      );
      await user.click(stop);
      await waitFor(() => expect(stop).not.toBeChecked());
      expect(
        screen.getByRole("switch", { name: "Merge when ready" }),
      ).toBeChecked();
      expect(requests[2]).toEqual({ stopVmAfterMerge: false });
    },
  );

  it("saves the Stop choice before allowing Ship to parent", async () => {
    usePreviewHandler();
    const requests: string[] = [];
    let releaseSave!: () => void;
    const saveGate = new Promise<void>((resolve) => {
      releaseSave = resolve;
    });
    const selected = pullRequestFixture({ stopVmAfterMerge: true });
    server.use(
      http.patch(
        "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
        async ({ request }) => {
          expect(await request.json()).toEqual({ stopVmAfterMerge: true });
          requests.push("save");
          await saveGate;
          return HttpResponse.json({ data: { pullRequest: selected } });
        },
      ),
      http.post("/api/v1/workspaces/ws-target/pull-requests/pr-1/merge", () => {
        requests.push("merge");
        return HttpResponse.json({
          data: {
            pullRequest: {
              ...selected,
              status: "merged",
              localMergeStatus: "merged",
            },
            run: {},
          },
        });
      }),
    );
    const user = userEvent.setup();
    renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: {
        ...PENDING_APPROVALS,
        outgoing: {
          ...PENDING_APPROVALS.outgoing!,
          mergeAdmission: { admitted: true, reason: null, requirements: [] },
        },
      },
    });
    const ship = screen.getByTestId("prepare-to-ship-ship-button");
    await user.click(
      screen.getByRole("switch", { name: "Stop VM after merge" }),
    );
    await waitFor(() => expect(requests).toEqual(["save"]));
    try {
      expect(ship).toBeDisabled();
    } finally {
      releaseSave();
    }
    await waitFor(() => expect(ship).toBeEnabled());
    expect(
      screen.getByRole("switch", { name: "Stop VM after merge" }),
    ).toBeChecked();
    expect(
      screen.getByRole("switch", { name: "Merge when ready" }),
    ).not.toBeChecked();
    await user.click(ship);
    await waitFor(() => expect(ship).toHaveTextContent("Shipped"));
    expect(requests).toEqual(["save", "merge"]);
  });

  it("shows a failed shutdown-setting save without selecting it or starting merge", async () => {
    usePreviewHandler();
    server.use(
      http.patch(
        "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
        () =>
          HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "The workspace is transferring.",
              },
            },
            { status: 409 },
          ),
      ),
    );
    const user = userEvent.setup();
    renderModal({ existingPullRequest: pullRequestFixture() });
    const stop = screen.getByRole("switch", { name: "Stop VM after merge" });
    await user.click(stop);
    await screen.findByText("The workspace is transferring.");
    expect(stop).not.toBeChecked();
    expect(
      screen.getByRole("switch", { name: "Merge when ready" }),
    ).not.toBeChecked();
  });

  it("offers Ship with agent for a blocked ship, with the dialog's agent choice, and opens the chat (g5.git.pr-auto-merge.with-agent)", async () => {
    usePreviewHandler();
    let sent: unknown = null;
    server.use(
      http.post(
        "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
        async ({ request }) => {
          sent = await request.json();
          return HttpResponse.json({
            data: {
              pullRequest: pullRequestFixture({
                autoMergeRequest: {
                  enabledBy: "u-dvd",
                  enabledAt: "2026-08-14T00:30:00Z",
                  mergeMethod: "REBASE",
                  reason: null,
                  checkedAt: null,
                },
              }),
              chatSessionId: "chat-loop",
            },
          });
        },
      ),
    );
    const onAgentStarted = vi.fn();
    const user = userEvent.setup();
    // Pending approvals block Ship to parent.
    renderModal({ existingPullRequest: pullRequestFixture(), onAgentStarted });
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    const agent = screen.getByTestId("prepare-to-ship-ship-with-agent");
    expect(agent).toHaveAttribute("data-emphasis", "primary");
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "Ship with agent resolves it",
    );
    // It waits for the launch resolution, like the rebase agent.
    await waitFor(() => expect(agent).toBeEnabled());
    await user.click(agent);
    await waitFor(() =>
      expect(onAgentStarted).toHaveBeenCalledWith("chat-loop"),
    );
    expect(sent).toMatchObject({
      agent: {
        agentLaunch: expect.objectContaining({
          confirmedResolutionId: "resolution-claude",
        }),
        requestKey: expect.any(String),
      },
    });
    expect(
      screen.getByTestId("prepare-to-ship-auto-merge-switch"),
    ).toBeChecked();
  });

  describe("Start and ship with agent (g5.chat.start-and-send, g5.git.pr-auto-merge.with-agent)", () => {
    type Kind = "stopped" | "failed" | "starting" | "live";
    const gateOf = (kind: Kind, owner = true) => ({
      kind,
      canWrite: owner,
      live: kind === "live",
      canStart: owner && (kind === "stopped" || kind === "failed"),
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner,
      workspaceNoun: "the workspace",
    });
    function renderWithGate(
      kind: Kind,
      owner: boolean,
      overrides: Partial<PrepareToShipModalProps> = {},
    ) {
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false },
          mutations: { retry: false },
        },
      });
      const props: PrepareToShipModalProps = {
        workspaceId: "ws-source",
        orgId: "org-1",
        sourceWorkspaceName: "Fork",
        targetWorkspaceId: "ws-target",
        targetWorkspaceName: "Base",
        repositoryComparison: null,
        shipApprovals: PENDING_APPROVALS,
        existingPullRequest: pullRequestFixture(),
        agentConfig: AGENT_CONFIG,
        resolveAgentLaunch: vi.fn(
          async (preferences: AgentLaunchPreferencesV1) =>
            resolvedAgentLaunch(preferences),
        ),
        onStartRebase: vi.fn(),
        onReviewApprovals: vi.fn(),
        onClose: vi.fn(),
        ...overrides,
      };
      serveApprovals(props.shipApprovals);
      return render(
        <QueryClientProvider client={queryClient}>
          <WorkspaceLiveGateProvider value={gateOf(kind, owner)}>
            <PrepareToShipModal {...props} />
          </WorkspaceLiveGateProvider>
        </QueryClientProvider>,
      );
    }

    it("reads Start and ship with agent on the owner's stopped VM, says what it does, and sends startCompute with the agent", async () => {
      usePreviewHandler();
      let sent: unknown = null;
      server.use(
        http.post(
          "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
          async ({ request }) => {
            sent = await request.json();
            return HttpResponse.json({
              data: {
                pullRequest: pullRequestFixture({
                  autoMergeRequest: {
                    enabledBy: "u-dvd",
                    enabledAt: "2026-11-05T00:30:00Z",
                    mergeMethod: "REBASE",
                    reason: null,
                    checkedAt: null,
                  },
                }),
                chatSessionId: "chat-loop",
                startCompute: { intentId: "intent-1", status: "starting" },
              },
            });
          },
        ),
      );
      const onAgentStarted = vi.fn();
      const user = userEvent.setup();
      renderWithGate("stopped", true, { onAgentStarted });
      const agent = screen.getByTestId("prepare-to-ship-ship-with-agent");
      expect(agent).toHaveTextContent("Start and ship with agent");
      expect(
        screen.getByTestId("prepare-to-ship-start-and-ship-note"),
      ).toHaveTextContent(
        "The workspace is stopped. Start and ship with agent starts it, turns on Merge when ready and queues the agent's first message, sent once it is running.",
      );
      await waitFor(() => expect(agent).toBeEnabled());
      await user.click(agent);
      await waitFor(() =>
        expect(onAgentStarted).toHaveBeenCalledWith("chat-loop"),
      );
      expect(sent).toMatchObject({
        agent: {
          agentLaunch: expect.objectContaining({
            confirmedResolutionId: "resolution-claude",
          }),
          requestKey: expect.any(String),
          startCompute: true,
        },
      });
    });

    it("retries a failed Start and ship with agent under the same request key, and takes a fresh key after a refusal", async () => {
      usePreviewHandler();
      const keys: string[] = [];
      let outcome: "fail" | "refuse" | "ok" = "fail";
      server.use(
        http.post(
          "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
          async ({ request }) => {
            const body = (await request.json()) as {
              agent: { requestKey: string };
            };
            keys.push(body.agent.requestKey);
            if (outcome === "fail") {
              return HttpResponse.json(
                { error: { code: "INTERNAL", message: "decision failed" } },
                { status: 500 },
              );
            }
            if (outcome === "refuse") {
              return HttpResponse.json(
                {
                  error: {
                    code: "CONFLICT",
                    message:
                      "The earlier Start ended before the agent's message was sent; press Ship with agent again",
                  },
                },
                { status: 409 },
              );
            }
            return HttpResponse.json({
              data: {
                pullRequest: pullRequestFixture({
                  autoMergeRequest: {
                    enabledBy: "u-dvd",
                    enabledAt: "2026-11-05T00:30:00Z",
                    mergeMethod: "REBASE",
                    reason: null,
                    checkedAt: null,
                  },
                }),
                chatSessionId: "chat-loop",
                startCompute: { intentId: "intent-1", status: "starting" },
              },
            });
          },
        ),
      );
      const onAgentStarted = vi.fn();
      const user = userEvent.setup();
      renderWithGate("stopped", true, { onAgentStarted });
      const agent = screen.getByTestId("prepare-to-ship-ship-with-agent");
      await waitFor(() => expect(agent).toBeEnabled());
      await user.click(agent);
      await waitFor(() => expect(keys).toHaveLength(1));
      await waitFor(() => expect(agent).toBeEnabled());
      // A failed attempt replays under the same key: the chat it may have
      // committed is reused, not duplicated.
      await user.click(agent);
      await waitFor(() => expect(keys).toHaveLength(2));
      expect(keys[1]).toBe(keys[0]);
      // A refusal ends that key: the next attempt is a fresh request.
      outcome = "refuse";
      await waitFor(() => expect(agent).toBeEnabled());
      await user.click(agent);
      await waitFor(() => expect(keys).toHaveLength(3));
      expect(keys[2]).toBe(keys[0]);
      outcome = "ok";
      await waitFor(() => expect(agent).toBeEnabled());
      await user.click(agent);
      await waitFor(() => expect(keys).toHaveLength(4));
      expect(keys[3]).not.toBe(keys[0]);
      await waitFor(() =>
        expect(onAgentStarted).toHaveBeenCalledWith("chat-loop"),
      );
    });

    it("stays Ship with agent while a Start is under way, showing its progress, and while running, sending no startCompute", async () => {
      usePreviewHandler();
      let sent: Record<string, unknown> | null = null;
      server.use(
        http.post(
          "/api/v1/workspaces/ws-target/pull-requests/pr-1/auto-merge",
          async ({ request }) => {
            sent = (await request.json()) as Record<string, unknown>;
            return HttpResponse.json({
              data: {
                pullRequest: pullRequestFixture(),
                chatSessionId: "chat-loop",
              },
            });
          },
        ),
      );
      const user = userEvent.setup();
      renderWithGate("starting", true);
      const starting = screen.getByTestId("prepare-to-ship-ship-with-agent");
      expect(starting).toHaveTextContent(/^Ship with agent$/);
      expect(screen.getByTestId("start-progress")).toHaveAttribute(
        "data-state",
        "starting",
      );
      expect(
        screen.queryByTestId("prepare-to-ship-start-and-ship-note"),
      ).toBeNull();
      await waitFor(() => expect(starting).toBeEnabled());
      await user.click(starting);
      await waitFor(() => expect(sent).not.toBeNull());
      expect(
        (sent as unknown as { agent: Record<string, unknown> }).agent,
      ).not.toHaveProperty("startCompute");
    });

    it("keeps the ordinary label and no note on a running VM, and for a non-owner on a stopped VM", () => {
      usePreviewHandler();
      const { unmount } = renderWithGate("live", true);
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toHaveTextContent(/^Ship with agent$/);
      expect(
        screen.queryByTestId("prepare-to-ship-start-and-ship-note"),
      ).toBeNull();
      expect(screen.queryByTestId("start-progress")).toBeNull();
      unmount();
      renderWithGate("stopped", false);
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toHaveTextContent(/^Ship with agent$/);
      expect(
        screen.queryByTestId("prepare-to-ship-start-and-ship-note"),
      ).toBeNull();
    });

    it("names a failed start in the note", () => {
      usePreviewHandler();
      renderWithGate("failed", true);
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toHaveTextContent("Start and ship with agent");
      expect(
        screen.getByTestId("prepare-to-ship-start-and-ship-note"),
      ).toHaveTextContent("The workspace failed to start.");
    });
  });

  it("always offers Ship with agent beside Ship to parent, secondary on the all-green path and unavailable without a PR", async () => {
    usePreviewHandler();
    const ready = renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: READY_APPROVALS,
    });
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeEnabled();
    const agent = screen.getByTestId("prepare-to-ship-ship-with-agent");
    expect(agent).toHaveAttribute("data-emphasis", "secondary");
    await waitFor(() => expect(agent).toBeEnabled());
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "Ready to merge.",
    );
    ready.unmount();

    usePreviewHandler();
    renderModal({ existingPullRequest: null, shipApprovals: READY_APPROVALS });
    expect(
      screen.getByTestId("prepare-to-ship-ship-with-agent"),
    ).toBeDisabled();
  });

  it("offers Ship with agent when the merge itself is refused", async () => {
    usePreviewHandler();
    server.use(
      http.post("/api/v1/workspaces/ws-target/pull-requests/pr-1/merge", () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "GitHub merge did not complete for all repos.",
            },
          },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: READY_APPROVALS,
    });
    await user.click(screen.getByTestId("prepare-to-ship-ship-button"));
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toBeInTheDocument(),
    );
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      "GitHub merge did not complete",
    );
  });

  it("shows a permission denial accurately and never claims Ship with agent resolves it (#1104, g5.workbench.ship-to-parent)", async () => {
    usePreviewHandler();
    server.use(
      http.post("/api/v1/workspaces/ws-target/pull-requests/pr-1/merge", () =>
        HttpResponse.json(
          {
            error: { code: "FORBIDDEN", message: "Insufficient permissions" },
          },
          { status: 403 },
        ),
      ),
    );
    const user = userEvent.setup();
    // A member owning the source, with no approval requirement on the parent:
    // Ship to parent is refused for permission, not a blocker an agent clears.
    renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: approvalsStateFixture([]),
    });
    await user.click(screen.getByTestId("prepare-to-ship-ship-button"));
    const note = await screen.findByText(/parent has an approval requirement/i);
    expect(note).toBeInTheDocument();
    expect(
      screen.getByTestId("prepare-to-ship-ship-note"),
    ).not.toHaveTextContent("Ship with agent resolves it");
    // Ship with agent is not styled as the resolution of a permission gap.
    expect(
      screen.getByTestId("prepare-to-ship-ship-with-agent"),
    ).toHaveAttribute("data-emphasis", "secondary");
  });

  it("disables Ship with agent for a reader of the source, who may still switch merge when ready on", async () => {
    usePreviewHandler();
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <WorkspaceLiveGateProvider
          value={{
            kind: "reader",
            canWrite: false,
            live: false,
            canStart: false,
            reviewCopy: false,
            reviewIdentityKnown: true,
            owner: false,
            workspaceNoun: "the workspace",
          }}
        >
          <PrepareToShipModal
            workspaceId="ws-source"
            orgId="org-1"
            sourceWorkspaceName="Fork"
            targetWorkspaceId="ws-target"
            targetWorkspaceName="Base"
            repositoryComparison={{
              repositories: [],
              hasBehind: false,
              hasUnknownBehind: false,
              allBehindKnownZero: true,
            }}
            shipApprovals={PENDING_APPROVALS}
            existingPullRequest={pullRequestFixture()}
            agentConfig={AGENT_CONFIG}
            resolveAgentLaunch={vi.fn(
              async (preferences: AgentLaunchPreferencesV1) =>
                resolvedAgentLaunch(preferences),
            )}
            onStartRebase={vi.fn()}
            onClose={vi.fn()}
          />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    // Even once the agent choice resolves, a reader cannot start the
    // source owner's agent.
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-update-codebase"),
      ).toBeInTheDocument(),
    );
    expect(
      screen.getByTestId("prepare-to-ship-ship-with-agent"),
    ).toBeDisabled();
    expect(
      screen.getByTestId("prepare-to-ship-auto-merge-switch"),
    ).toBeEnabled();
    expect(
      screen.getByRole("switch", { name: "Stop VM after merge" }),
    ).toBeDisabled();
  });

  it("offers no merge when ready without an open pull request", () => {
    usePreviewHandler();
    renderModal({ existingPullRequest: null });
    expect(
      screen.queryByTestId("prepare-to-ship-auto-merge"),
    ).not.toBeInTheDocument();
  });

  it("names the description textarea and links the toggle to it", async () => {
    usePreviewHandler();
    const user = userEvent.setup();
    renderModal();
    const toggle = screen.getByTestId("prepare-to-ship-description-toggle");
    expect(toggle).toHaveAttribute(
      "aria-controls",
      "prepare-to-ship-description",
    );
    await user.click(toggle);
    expect(
      screen.getByRole("textbox", { name: "PR description" }),
    ).toBeInTheDocument();
  });

  it("disables the header close button while the ship merge is in flight", async () => {
    usePreviewHandler();
    let releaseMerge: () => void = () => {};
    const mergeGate = new Promise<void>((resolve) => {
      releaseMerge = resolve;
    });
    server.use(
      http.post(
        "/api/v1/workspaces/ws-target/pull-requests/pr-1/merge",
        async () => {
          await mergeGate;
          return HttpResponse.json({
            data: {
              pullRequest: pullRequestFixture({
                status: "merged",
                localMergeStatus: "merged",
                mergedAt: "2026-08-14T01:00:00Z",
              }),
              run: {},
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: READY_APPROVALS,
    });
    const closeButton = screen.getByTitle("Close");
    expect(closeButton).toBeEnabled();
    await user.click(screen.getByTestId("prepare-to-ship-ship-button"));
    await waitFor(() => expect(closeButton).toBeDisabled());
    releaseMerge();
    await waitFor(() =>
      expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
        "Merged into Base.",
      ),
    );
    expect(closeButton).toBeEnabled();
  });

  it("sends an explicit blank to clear a stored override when the textarea is emptied", async () => {
    const createBodies: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/ws-source/pull-requests",
        async ({ request }) => {
          createBodies.push(await request.json());
          return HttpResponse.json({
            data: {
              pullRequest: pullRequestFixture({ descriptionOverride: null }),
              alreadyExisted: true,
              dirtyDrafts: [],
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderModal({
      existingPullRequest: pullRequestFixture({
        descriptionOverride: "stored override",
      }),
    });
    await user.click(screen.getByTestId("prepare-to-ship-description-toggle"));
    await user.clear(screen.getByTestId("prepare-to-ship-description"));
    await user.click(screen.getByTestId("prepare-to-ship-create"));
    await waitFor(() => expect(createBodies).toHaveLength(1));
    // Update PR sends `mode: update` and saves first (g5d sends `save: false`).
    expect(createBodies[0]).toEqual({
      targetWorkspaceId: "ws-target",
      description: "",
      mode: "update",
    });
  });

  it("sends an edited description to an existing GitHub PR, like g5d pr update --body (g5.governance.pr.body)", async () => {
    const createBodies: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/ws-source/pull-requests",
        async ({ request }) => {
          createBodies.push(await request.json());
          return HttpResponse.json({
            data: {
              pullRequest: pullRequestFixture({
                descriptionOverride: "Reviewed body",
              }),
              alreadyExisted: true,
              dirtyDrafts: [],
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderModal({
      existingPullRequest: pullRequestFixture({
        descriptionOverride: "stored override",
        providerRepos: [
          {
            id: "provider-code",
            repoId: "repo-code",
            provider: "github",
            providerRepoFullName: "G5Labs/G5",
            sourceRemoteBranch: "g5/fork",
            targetRemoteBranch: "main",
            providerPrNumber: 77,
            providerPrUrl: "https://github.com/G5Labs/G5/pull/77",
            providerStatus: "open",
            sourceHeadSha: null,
            targetHeadSha: null,
            providerHeadSha: null,
            providerBaseSha: null,
            lastError: null,
            lastSyncedAt: null,
            updatedAt: "2026-08-14T00:00:00Z",
          },
        ],
      }),
    });
    expect(
      screen.getByTestId("prepare-to-ship-description-toggle"),
    ).toHaveTextContent(/unchanged on github unless edited/i);
    await user.click(screen.getByTestId("prepare-to-ship-description-toggle"));
    const textarea = screen.getByTestId("prepare-to-ship-description");
    expect(textarea).toBeEnabled();
    await user.clear(textarea);
    await user.type(textarea, "Reviewed body");
    await user.click(screen.getByTestId("prepare-to-ship-create"));
    await waitFor(() => expect(createBodies).toHaveLength(1));
    expect(createBodies[0]).toEqual({
      targetWorkspaceId: "ws-target",
      description: "Reviewed body",
      mode: "update",
    });
  });

  it("uses the stored description override as the baseline when the PR already carries one", () => {
    // No preview handler on purpose: a stored override suppresses the call.
    renderModal({
      existingPullRequest: pullRequestFixture({
        descriptionOverride: "stored override",
      }),
    });
    expect(
      screen.getByTestId("prepare-to-ship-description-toggle"),
    ).toHaveTextContent(/stored description/i);
  });
});

describe("PrepareToShipModal — merge with override (g5.workbench.ship-to-parent)", () => {
  const OVERRIDE_URL =
    "/api/v1/workspaces/ws-target/pull-requests/pr-1/merge-override";

  function overridePreview(
    overrides: Partial<MergeOverridePreview> = {},
  ): MergeOverridePreview {
    return {
      sourceHeads: [],
      blockers: {
        approvals: [
          {
            definitionId: "def-1",
            definitionName: "Double signoff",
            definitionVersion: 1,
            state: "pending",
          },
        ],
        checks: [],
        github: [],
      },
      notOverridable: [],
      authority: { g5: { allowed: true }, github: { allowed: true } },
      selfOverride: false,
      digest: "digest-1",
      ...overrides,
    };
  }

  function serveOverridePreview(preview: MergeOverridePreview) {
    let reads = 0;
    server.use(
      http.get(OVERRIDE_URL, () => {
        reads += 1;
        return HttpResponse.json({ data: preview });
      }),
    );
    return () => reads;
  }

  it("offers Merge with override beside Ship to parent while blocked, and ships with it", async () => {
    usePreviewHandler();
    serveOverridePreview(overridePreview());
    let sent: unknown = null;
    server.use(
      http.post(OVERRIDE_URL, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({
          data: {
            pullRequest: pullRequestFixture({
              status: "merged",
              localMergeStatus: "merged",
              mergedAt: "2026-10-27T00:00:00Z",
              mergedBy: "u-admin",
              mergeOverride: {
                actorId: "u-admin",
                actorName: "Ada Admin",
                at: "2026-10-27T00:00:00Z",
                reason: "Approver on leave",
                selfOverride: true,
                waivedApprovals: overridePreview().blockers.approvals,
                waivedChecks: [],
                githubRepositories: [],
              },
            }),
            run: { id: "run-1" },
          },
        });
      }),
    );
    const user = userEvent.setup();
    renderModal({ existingPullRequest: pullRequestFixture() });

    const override = await screen.findByTestId(
      "prepare-to-ship-merge-override",
    );
    // Beside Ship to parent, never replacing it.
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    expect(override).toHaveTextContent("Merge with override…");

    await user.click(override);
    const dialog = await screen.findByTestId("merge-override-dialog");
    await within(dialog).findByTestId("merge-override-approval-def-1");
    await user.type(
      within(dialog).getByTestId("merge-override-reason"),
      "Approver on leave",
    );
    await user.click(
      within(dialog).getByTestId("merge-override-confirm-checkbox"),
    );
    await user.click(within(dialog).getByTestId("merge-override-confirm"));

    await waitFor(() =>
      expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
        "Merged with override by Ada Admin: Approver on leave (self-override)",
      ),
    );
    expect(sent).toEqual({
      reason: "Approver on leave",
      confirmed: true,
      digest: "digest-1",
    });
    expect(screen.queryByTestId("merge-override-dialog")).toBeNull();
    expect(screen.queryByTestId("prepare-to-ship-merge-override")).toBeNull();
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toHaveTextContent(
      "Shipped",
    );
  });

  it("offers it for GitHub rules alone when only the GitHub group is granted", async () => {
    usePreviewHandler();
    serveOverridePreview(
      overridePreview({
        blockers: {
          approvals: [],
          checks: [],
          github: [
            {
              repoId: "repo-code",
              repository: "acme/web",
              headSha: "abc",
              mergeState: "blocked",
            },
          ],
        },
        authority: {
          g5: { allowed: false, reason: "Needs an organization admin." },
          github: { allowed: true },
        },
      }),
    );
    renderModal({ existingPullRequest: pullRequestFixture() });
    expect(
      await screen.findByTestId("prepare-to-ship-merge-override"),
    ).toBeInTheDocument();
  });

  it("offers no override when the server denies a group that is present, showing its reason", async () => {
    usePreviewHandler();
    const reads = serveOverridePreview(
      overridePreview({
        authority: {
          g5: {
            allowed: false,
            reason: "Only an organization administrator may override.",
          },
          github: { allowed: true },
        },
      }),
    );
    renderModal({ existingPullRequest: pullRequestFixture() });
    expect(
      await screen.findByTestId("prepare-to-ship-merge-override-denied"),
    ).toHaveTextContent("Only an organization administrator may override.");
    expect(reads()).toBeGreaterThan(0);
    expect(screen.queryByTestId("prepare-to-ship-merge-override")).toBeNull();
  });

  it("offers no override when nothing overridable blocks", async () => {
    usePreviewHandler();
    const reads = serveOverridePreview(
      overridePreview({
        blockers: { approvals: [], checks: [], github: [] },
        notOverridable: ["The source has a merge conflict."],
      }),
    );
    renderModal({
      existingPullRequest: pullRequestFixture({ localMergeStatus: "conflict" }),
      shipApprovals: READY_APPROVALS,
    });
    await waitFor(() => expect(reads()).toBeGreaterThan(0));
    expect(screen.queryByTestId("prepare-to-ship-merge-override")).toBeNull();
    expect(
      screen.queryByTestId("prepare-to-ship-merge-override-denied"),
    ).toBeNull();
  });

  it("neither offers nor reads the override while Ship to parent is available", async () => {
    usePreviewHandler();
    const reads = serveOverridePreview(overridePreview());
    renderModal({
      existingPullRequest: pullRequestFixture(),
      shipApprovals: READY_APPROVALS,
    });
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeEnabled();
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toBeEnabled(),
    );
    expect(screen.queryByTestId("prepare-to-ship-merge-override")).toBeNull();
    expect(reads()).toBe(0);
  });

  it("does not read the override without an open pull request", async () => {
    usePreviewHandler();
    const reads = serveOverridePreview(overridePreview());
    renderModal({ existingPullRequest: null });
    await screen.findByTestId("prepare-to-ship-ship-note");
    expect(screen.queryByTestId("prepare-to-ship-merge-override")).toBeNull();
    expect(reads()).toBe(0);
  });
});

describe("PrepareToShipModal — external-check merge policy", () => {
  const PROVIDER_PR = {
    id: "provider-code",
    repoId: "repo-code",
    provider: "github",
    providerRepoFullName: "G5Labs/G5",
    sourceRemoteBranch: "g5/fork",
    targetRemoteBranch: "main",
    providerPrNumber: 77,
    providerPrUrl: "https://github.com/G5Labs/G5/pull/77",
    providerStatus: "open" as const,
    sourceHeadSha: null,
    targetHeadSha: null,
    providerHeadSha: null,
    providerBaseSha: null,
    lastError: null,
    lastSyncedAt: null,
    updatedAt: "2026-08-14T00:00:00Z",
  };

  function serveChecks(repositories: unknown[]) {
    server.use(
      http.get("/api/v1/workspaces/ws-source/pull-requests/info", () =>
        HttpResponse.json({
          data: { source: "github", repositories },
        }),
      ),
    );
  }

  function serveKnob(enabled: boolean) {
    server.use(
      http.get("/api/v1/orgs/org-1/config", () =>
        HttpResponse.json({
          data: { settings: { external_checks_block_merge: enabled } },
        }),
      ),
    );
  }

  const FAILING = [
    {
      repository: "G5Labs/G5",
      number: 77,
      availability: "ready",
      checks: [{ kind: "check_run", name: "ci", bucket: "fail" }],
    },
  ];

  it("blocks Ship when the org requires passing checks and they are failing", async () => {
    usePreviewHandler();
    serveKnob(true);
    serveChecks(FAILING);
    renderModal({
      shipApprovals: null,
      existingPullRequest: pullRequestFixture({ providerRepos: [PROVIDER_PR] }),
    });

    // The checks section renders the failing verdict.
    await waitFor(() =>
      expect(screen.getByTestId("external-pr-checks")).toHaveAttribute(
        "data-verdict",
        "failing",
      ),
    );
    expect(screen.getByTestId("external-pr-checks-verdict")).toHaveTextContent(
      "Failing",
    );

    // Ship is blocked with the policy reason even though localMergeStatus=ready,
    // and an agent is offered to fix it.
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toBeInTheDocument(),
    );
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      /external checks are failing/i,
    );
  });

  it("splits required from other checks and waits only on required ones", async () => {
    usePreviewHandler();
    serveKnob(true);
    serveChecks([
      {
        repository: "G5Labs/G5",
        number: 77,
        availability: "ready",
        baseRefName: "main",
        checks: [
          {
            kind: "expected",
            name: "Cloud Build",
            bucket: "pending",
            required: true,
          },
          {
            kind: "check_run",
            name: "slow review",
            bucket: "pending",
            required: false,
          },
        ],
      },
    ]);
    renderModal({
      shipApprovals: null,
      existingPullRequest: pullRequestFixture({ providerRepos: [PROVIDER_PR] }),
    });

    await waitFor(() =>
      expect(screen.getByTestId("external-pr-checks")).toHaveAttribute(
        "data-verdict",
        "pending",
      ),
    );
    const required = screen.getByTestId("external-pr-checks-required");
    expect(required).toHaveTextContent("Cloud Build");
    expect(required).toHaveTextContent(/expected, not reported yet/i);
    expect(required).not.toHaveTextContent("slow review");
    expect(
      screen.getByTestId("external-pr-checks-not-required"),
    ).toHaveTextContent("slow review");
    await waitFor(() =>
      expect(
        screen.getByTestId("prepare-to-ship-ship-with-agent"),
      ).toBeInTheDocument(),
    );
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      /required external checks have not finished/i,
    );
  });

  it("passes when the base branch requires nothing and nothing failed", async () => {
    usePreviewHandler();
    serveKnob(true);
    serveChecks([
      {
        repository: "G5Labs/G5",
        number: 77,
        availability: "ready",
        baseRefName: "main",
        checks: [
          {
            kind: "check_run",
            name: "slow review",
            bucket: "pending",
            required: false,
          },
        ],
      },
    ]);
    renderModal({
      shipApprovals: null,
      existingPullRequest: pullRequestFixture({ providerRepos: [PROVIDER_PR] }),
    });

    await waitFor(() =>
      expect(screen.getByTestId("external-pr-checks")).toHaveAttribute(
        "data-verdict",
        "passing",
      ),
    );
    expect(screen.getByTestId("external-pr-checks-required")).toHaveTextContent(
      /main requires no checks/i,
    );
    expect(
      screen.getByTestId("prepare-to-ship-ship-button"),
    ).not.toBeDisabled();
  });

  it("names a local repository pull request by its G5 PR, never as #N", async () => {
    // g5.git.pr-local-number: a G5 PR's number is written PR######.
    usePreviewHandler();
    serveKnob(true);
    serveChecks([
      {
        repository: "G5Labs/G5",
        number: 77,
        availability: "ready",
        baseRefName: "main",
        checks: [],
      },
      {
        repository: "docs",
        tracker: "g5",
        number: null,
        g5Number: "PR000007",
        availability: "ready",
        noExternalChecks: true,
        message: "No external checks apply: this repository is not on GitHub.",
        checks: [],
      },
    ]);
    renderModal({
      shipApprovals: null,
      existingPullRequest: pullRequestFixture({ providerRepos: [PROVIDER_PR] }),
    });

    const section = await screen.findByTestId("external-pr-checks");
    await waitFor(() => expect(section).toHaveTextContent("docs (PR000007)"));
    expect(section).toHaveTextContent("G5Labs/G5 #77");
    expect(section).not.toHaveTextContent(/G5 PR|#7\b/);
  });

  it("shows checks advisorily but does not block Ship when the policy is off", async () => {
    usePreviewHandler();
    serveKnob(false);
    serveChecks(FAILING);
    renderModal({
      shipApprovals: null,
      existingPullRequest: pullRequestFixture({ providerRepos: [PROVIDER_PR] }),
    });

    await waitFor(() =>
      expect(screen.getByTestId("external-pr-checks")).toHaveAttribute(
        "data-verdict",
        "failing",
      ),
    );
    // Advisory: the Ship button stays enabled on a ready PR.
    expect(
      screen.getByTestId("prepare-to-ship-ship-button"),
    ).not.toBeDisabled();
    expect(screen.getByText("Advisory")).toBeInTheDocument();
  });
});
