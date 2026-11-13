// =============================================================================
// WorkspacePage — workbench shell (ui g5.ui.workbench-shell)
// =============================================================================
// The PhaseChevrons and ActionRibbon head the page, the chat header carries
// the phase pill, and a chevron click PATCHes the owner's phase
// (g5.workbench.phases.state).
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  renderHook,
  screen,
  waitFor,
  within,
  fireEvent,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { workbenchConfigKey } from "../hooks/useWorkbench";
import { useUpdateWorkspacePhase } from "../hooks/useWorkspace";
import { ribbonButtonShape } from "../components/workbench/ActionRibbon";
import { BASE_ISSUE_LINK_REASON } from "../components/workspace/WorkspaceDetailsPanel";
import { GETTING_STARTED_AUTO_OPENED_STORAGE_KEY } from "../lib/userGuide";
import { findWorkbenchAction } from "../lib/journeySteps";
import { WORKBENCH_ACTIONS } from "../lib/workbenchDefaults";
import { workbenchActionMessage } from "../lib/workbenchMessage";
import {
  reviewWorkspaceChatPrompt,
  VERIFICATION_NOTICE,
  verificationChatPrompt,
} from "../lib/reviewPrompts";
import {
  VERIFICATION_ACTION_ENABLED_SETTING,
  VERIFICATION_ACTION_OFF_TITLE,
} from "../lib/verificationActionFlags";
import { RIBBON_FALLBACK_CHAT_TITLE } from "../components/workbench/ActionRibbon";
import type {
  ActivityEntryDTO,
  CommitDiffResponse,
  JourneyMarkDTO,
  ReviewComment,
  WorkspaceActivityDTO,
} from "../lib/types";
import {
  createMockDocNode,
  createMockTask,
  createMockUser,
  createMockWorkspaceVm,
  mockInstanceInfo,
  mockWorkspaceDetails,
  mockWorkspaces,
} from "../test/fixtures";
import { MOCK_WORKSPACE_ACTIVITY } from "../test/fixtures";
import { seedMockWorkspaceApprovals } from "../test/msw/handlers";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import { AppShell } from "../components/layout/Layout";
import { WorkspaceTabsProvider } from "../hooks/useWorkspaceTabs";
import { workspaceTabsKey } from "../lib/workspaceTabs";
import {
  issueConflictResponse,
  makeIssueStateChange,
  serveGithubConnection,
  makeIssueDetail,
  makeIssueLink,
  makeNativeIssueDetail,
  serveNativeIssuesFeature,
  serveOrgExternalIssues,
} from "../test/externalIssueFixtures";
import { startMessageFromIssue } from "../lib/startMessageFromIssue";
import { IssuePage } from "./IssuePage";
import { WorkspacePage } from "./WorkspacePage";

// These page tests exercise navigation; real terminal connectivity belongs
// to the VM/browser acceptance cases.
vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div data-testid="stub-workspace-terminal" />,
}));

// The center tab bar is hidden by default (workspaces
// g5.workspace.ui.center-tabs); this suite drives the tabs, so it opts in.
// The suite drives the stacked sidebar, which is no longer the default
// (g5.ui.sidebar-default).
beforeEach(() =>
  seedUserSettings({ showTabBar: true, sidebarDisplayMode: "expanded" }),
);

function installWorkspace(options: {
  features?: string[];
  isBase?: boolean;
  ownerUserId?: string;
  phase?: "be" | "se" | "ie";
  /** Refuse every PATCH with this error (the API's envelope shape). */
  rejectPatch?: { status: number; message: string };
  /** Hold every PATCH response until this settles (a move in flight). */
  holdPatch?: Promise<void>;
  /** Extra detail fields every GET/PATCH reports (e.g. `docsFolderPath`). */
  detail?: Record<string, unknown>;
}) {
  const patches: unknown[] = [];
  // The durable phase: a PATCH changes what later GETs report, like the API.
  let currentPhase: "be" | "se" | "ie" = options.phase ?? "be";
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...mockInstanceInfo,
          features: [...mockInstanceInfo.features, ...(options.features ?? [])],
        },
      }),
    ),
    http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          isBase: options.isBase ?? false,
          ownerUserId: options.ownerUserId ?? "user-alex",
          readOnly: false,
          phase: currentPhase,
          ...(options.detail ?? {}),
        },
      }),
    ),
    http.patch("/api/v1/workspaces/:id", async ({ params, request }) => {
      const body = (await request.json()) as { phase?: "be" | "se" | "ie" };
      patches.push(body);
      if (options.holdPatch) await options.holdPatch;
      if (options.rejectPatch) {
        return HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: options.rejectPatch.message,
              details: {},
            },
          },
          { status: options.rejectPatch.status },
        );
      }
      if (body.phase) currentPhase = body.phase;
      return HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          isBase: false,
          ownerUserId: options.ownerUserId ?? "user-alex",
          phase: currentPhase,
          ...(options.detail ?? {}),
        },
      });
    }),
  );
  return patches;
}

/**
 * The slug resolver maps `test-workspace` to the first mock workspace
 * (`base`), which is the id the page's approvals query uses. A BE+IE
 * definition: BE approved, IE pending, so the server refuses admission.
 */
function seedPhasedApprovals(submitted = true) {
  server.use(
    http.get("/api/v1/workspaces/:id/review-flow", () =>
      HttpResponse.json({
        data: {
          workspace: {
            ownerUserId: "user-alex",
            targetWorkspaceId: "parent-1",
            phase: "se",
            readOnly: false,
          },
          pr: submitted
            ? {
                id: "phase-pr",
                title: "Phased review",
                description: null,
                sourceHeads: [],
                createdAt: "2026-09-01T09:00:00Z",
              }
            : null,
          heads: [],
          baseHeads: null,
          parentHeads: null,
          pushedAt: {},
          repos: [],
          requests: [],
          decisions: [],
        },
      }),
    ),
  );
  const definition = {
    id: "apprdef-phased",
    latestVersion: 1,
    name: "Sign-off",
    description: null,
    humans: [
      {
        userId: "user-alex",
        displayName: "Alex Chen",
        email: "alex@g5labs.dev",
        avatarUrl: null,
        stillMember: true,
      },
    ],
    agentType: null,
    agentPrompt: null,
    invalidatesPrevious: false,
    phases: ["be", "ie"] as ("be" | "se" | "ie")[],
    createdBy: { userId: "user-alex", displayName: "Alex Chen" },
    createdAt: "2026-08-11T09:00:00.000Z",
    firstCreatedBy: { userId: "user-alex", displayName: "Alex Chen" },
    firstCreatedAt: "2026-07-28T09:00:00.000Z",
  };
  const approver = (state: "approved" | "pending") => ({
    userId: "user-alex",
    displayName: "Alex Chen",
    email: "alex@g5labs.dev",
    stillMember: true,
    state,
    comment: null,
    decidedAt: state === "approved" ? "2026-09-01T10:00:00Z" : null,
    requestId: null,
  });
  seedMockWorkspaceApprovals("base", {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: {
      targetWorkspaceId: "parent-1",
      targetWorkspaceName: "parent",
      requirements: [
        {
          definition,
          phase: "ie",
          state: "pending",
          approvers: [approver("pending")],
          callerCanDecide: true,
          decisionLog: [],
        },
      ],
      mergeAdmission: {
        admitted: false,
        reason: "Approval(s) required before merging",
        requirements: [
          {
            definitionId: "apprdef-phased",
            definitionName: "Sign-off",
            definitionVersion: 1,
            state: "pending",
          },
        ],
      },
    },
  });
}

function renderWorkspace(
  route = "/org-g5labs/test-workspace",
  queryClient?: QueryClient,
) {
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    { route, user: createMockUser(), queryClient },
  );
}

/** The workspace the `test-workspace` slug resolves to (msw `/resolve`). */
const RESOLVED_WORKSPACE_ID = mockWorkspaces[0].id;

/**
 * Start a phase move from "another page" — a hook instance on a shared
 * QueryClient that is unmounted before the workspace page renders, the way
 * a Home chip files its PATCH and navigates (g5.workbench.phases.state).
 */
function startMoveElsewhere(phase: "be" | "se" | "ie") {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
  const starter = renderHook(
    () => useUpdateWorkspacePhase(RESOLVED_WORKSPACE_ID),
    { wrapper },
  );
  act(() => {
    starter.result.current.mutate(phase);
  });
  starter.unmount();
  return queryClient;
}

const WORKBENCH_REVISION = "mock-head.builtin0000";

const RESOLVED_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "wb-confirmed",
  configRevision: "test",
  catalogVersion: "test",
  provider: { value: "claude_code", provenance: "configured" },
  providerRoute: "anthropic_direct",
  model: { value: "configured-model", provenance: "configured" },
  upstreamModel: "configured-model",
  permissionMode: { value: "auto", provenance: "configured" },
  effort: { value: "high", provenance: "configured" },
  authMode: "anthropic_api_key",
  egressMode: "direct_provider",
  credentialReady: true,
  requiresInteractiveAuth: false,
};

function mockSession(id: string, persona: "be" | "se" | "ie" | null) {
  return {
    id,
    workspaceId: "base",
    scopeType: "workspace",
    scopeRef: "base",
    model: "configured-model",
    agentProvider: "claude_code",
    title: `Session ${id}`,
    summary: null,
    tags: [],
    messageCount: 2,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalCostUsd: 0,
    forkedFromId: null,
    linkedCommits: [],
    createdAt: "2026-10-06T00:00:00Z",
    updatedAt: "2026-10-06T00:00:00Z",
    canSend: true,
    persona,
  };
}

/**
 * Workbench Phase 2 surfaces (g5.workbench.actions, g5.workbench.persona):
 * a PR target so the ship surface is reachable, the launch resolution a
 * templated start needs, and recorders for the chat start and fork requests.
 */
function installWorkbench(
  options: {
    sessionPersona?: "be" | "se" | "ie" | null;
  } = {},
) {
  const starts: Record<string, unknown>[] = [];
  const forks: Record<string, unknown>[] = [];
  const resolves: Record<string, unknown>[] = [];
  const sessions = new Map<string, ReturnType<typeof mockSession>>();
  if (options.sessionPersona !== undefined) {
    sessions.set(
      "chat-tgr-1",
      mockSession("chat-tgr-1", options.sessionPersona),
    );
  }
  server.use(
    http.get("/api/v1/workspaces/:id/pull-requests/targets", () =>
      HttpResponse.json({ data: [{ id: "parent-1", name: "parent" }] }),
    ),
    http.post(
      "/api/v1/workspaces/:id/config/agent-launch/resolve",
      async ({ request }) => {
        resolves.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } });
      },
    ),
    http.get("/api/v1/workspaces/:id/repos/:repoId/preview-applications", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.post("/api/v1/workspaces/:id/sessions/start", async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      starts.push(body);
      const session = mockSession(
        `session-wb-${starts.length}`,
        (body.persona as "be" | "se" | "ie" | undefined) ?? null,
      );
      sessions.set(session.id, session);
      return HttpResponse.json(
        {
          data: {
            session,
            message: {
              messageId: "in-1",
              status: "queued",
              createdAt: "2026-10-06T00:00:00Z",
            },
            assistantMessageId: "in-1",
            taskId: "task-wb",
          },
        },
        { status: 201 },
      );
    }),
    http.post("/api/v1/sessions/:sessionId/fork", async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      forks.push(body);
      const session = mockSession(
        `session-fork-${forks.length}`,
        (body.persona as "be" | "se" | "ie" | undefined) ?? null,
      );
      sessions.set(session.id, session);
      return HttpResponse.json(
        { data: { sessionId: session.id, messagesCopied: 2 } },
        { status: 201 },
      );
    }),
    http.get("/api/v1/sessions/:sessionId/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/sessions/:sessionId", ({ params }) => {
      const session = sessions.get(String(params.sessionId));
      return session
        ? HttpResponse.json({ data: session })
        : HttpResponse.json({
            data: mockSession(String(params.sessionId), null),
          });
    }),
  );
  return { starts, forks, resolves };
}

beforeEach(() => {
  localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
  vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
    matches: query.includes("min-width: 1024px"),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
});

describe("WorkspacePage — alignment layout (g5.workbench.gate)", () => {
  it("renders the chevrons, ribbon and chat pill instead of the status band and ReposBar", async () => {
    installWorkspace({ phase: "se" });
    // `?session=` opens the chat right panel whose header carries the pill.
    renderWorkspace("/org-g5labs/test-workspace?session=chat-tgr-1");

    const chevrons = await screen.findByTestId("phase-chevrons");
    expect(chevrons).toHaveAttribute("data-phase", "se");
    expect(screen.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    const ribbon = screen.getByTestId("action-ribbon");
    expect(ribbon).toHaveAttribute("data-phase", "se");
    // Phase 2: ribbon actions are live once the served configuration has
    // loaded, which can land a render after the chevrons.
    await waitFor(() =>
      expect(screen.getByTestId("ribbon-action-generate-code")).toBeEnabled(),
    );
    expect(screen.queryByTestId("workspace-status-band")).toBeNull();
    expect(screen.queryByTestId("repos-bar")).toBeNull();
    // Chat header scope pill carries the phase abbreviation.
    const pill = await screen.findByTestId("chat-phase-pill");
    expect(pill).toHaveTextContent("SE");
    expect(pill).toHaveAttribute("data-phase", "se");
    // Default header styles (g5.workbench.header-styles): minimal chevrons
    // over a dynamic ribbon, stacked, with no branch chip.
    const header = screen.getByTestId("workbench-header");
    expect(header).toHaveAttribute("data-phase-bar", "minimal");
    expect(header).toHaveAttribute("data-action-bar", "dynamic");
    expect(chevrons).toHaveAttribute("data-variant", "minimal");
    expect(ribbon).toHaveAttribute("data-variant", "dynamic");
    expect(screen.queryByText("Design doc + prototype")).toBeNull();
    expect(screen.queryByTestId("ribbon-branch")).toBeNull();
  });

  // g5.workbench.header-styles: the stored Phase bar / Action bar settings
  // pick the variants; the vertical column shares the row with the ribbon,
  // icon-only buttons keep their label as the accessible name, and the
  // owner's move still PATCHes from the column.
  it("renders the vertical phase column beside an icon-only ribbon from the settings and still moves the phase", async () => {
    const patches = installWorkspace({ phase: "be" });
    seedUserSettings({
      phaseBarStyle: "vertical",
      actionBarStyle: "minimal-icons",
    });
    renderWorkspace();

    const chevrons = await screen.findByTestId("phase-chevrons");
    const header = screen.getByTestId("workbench-header");
    expect(header).toHaveAttribute("data-phase-bar", "vertical");
    expect(header).toHaveAttribute("data-action-bar", "minimal-icons");
    expect(header.className).toContain("flex-row");
    expect(chevrons).toHaveAttribute("data-variant", "vertical");
    expect(chevrons).toHaveTextContent("Business Engineering");
    expect(chevrons).toHaveTextContent("Integration Engineering");
    expect(chevrons).not.toHaveTextContent("Requirement doc");
    const ribbon = screen.getByTestId("action-ribbon");
    expect(ribbon).toHaveAttribute("data-variant", "minimal-icons");
    await waitFor(() =>
      expect(screen.getByTestId("ribbon-action-build-mock")).toBeEnabled(),
    );
    const build = screen.getByRole("button", { name: "Build mock" });
    expect(build).toHaveAttribute("title", "Build mock");
    expect(build).toHaveTextContent("");
    expect(screen.queryByText("Author")).toBeNull();
    expect(screen.queryByTestId("ribbon-branch")).toBeNull();

    await userEvent.click(screen.getByTestId("phase-chevron-se"));
    await waitFor(() => expect(patches).toEqual([{ phase: "se" }]));
    expect(screen.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  // g5.workbench.header-styles: `dynamic` renders the extended-icons shape
  // at full size in jsdom (no layout), scrolls instead of wrapping, and the
  // workspace controls take the ribbon's rendered style.
  it("renders the dynamic action bar with the workspace controls at full size", async () => {
    installWorkspace({ phase: "be" });
    seedUserSettings({ actionBarStyle: "dynamic" });
    renderWorkspace();

    const ribbon = await screen.findByTestId("action-ribbon");
    expect(screen.getByTestId("workbench-header")).toHaveAttribute(
      "data-action-bar",
      "dynamic",
    );
    expect(ribbon).toHaveAttribute("data-variant", "dynamic");
    expect(ribbon).toHaveAttribute("data-density", "full");
    expect(screen.getByTestId("ribbon-groups").className).toContain(
      "overflow-x-auto",
    );
    expect(screen.getByTestId("ribbon-action-build-mock")).toHaveTextContent(
      "Build mock",
    );
    const controls = within(screen.getByTestId("ribbon-trailing")).getByTestId(
      "workbench-workspace-controls",
    );
    expect(
      within(controls).getByTestId("workspace-info-button").className,
    ).toBe(ribbonButtonShape("extended-icons", {}).className);
    expect(screen.getByTestId("ribbon-workspace-approvals").className).toBe(
      ribbonButtonShape("extended-icons", {}).className,
    );
  });

  it("PATCHes the phase when the owner clicks a chevron and reflects it at once", async () => {
    const patches = installWorkspace({});
    renderWorkspace();

    await screen.findByTestId("phase-chevrons");
    await userEvent.click(screen.getByTestId("phase-chevron-se"));

    await waitFor(() => expect(patches).toEqual([{ phase: "se" }]));
    await waitFor(() =>
      expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
        "data-phase",
        "se",
      ),
    );
    expect(screen.getByTestId("action-ribbon")).toHaveAttribute(
      "data-phase",
      "se",
    );
  });

  it("keeps the chevrons read-only for a non-owner", async () => {
    const patches = installWorkspace({
      ownerUserId: "user-somebody-else",
      phase: "ie",
    });
    renderWorkspace();

    const target = await screen.findByTestId("phase-chevron-be");
    expect(target).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(target);
    expect(patches).toEqual([]);
    expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
      "data-phase",
      "ie",
    );
  });

  it("shows Base read-only at Business Engineering", async () => {
    const patches = installWorkspace({
      isBase: true,
      phase: "se",
    });
    renderWorkspace();

    const chevrons = await screen.findByTestId("phase-chevrons");
    expect(chevrons).toHaveAttribute("data-phase", "be");
    const target = screen.getByTestId("phase-chevron-se");
    expect(target).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(target);
    expect(patches).toEqual([]);
    // Base offers no template actions (g5.workbench.actions): its ribbon row
    // carries only the workspace controls (g5.workbench.shell-chrome).
    const ribbon = screen.getByTestId("action-ribbon");
    expect(within(ribbon).queryAllByRole("group")).toHaveLength(0);
    expect(within(ribbon).getByTestId("workspace-info-button")).toBeVisible();
    expect(within(ribbon).queryByTestId("workspace-owner-trigger")).toBeNull();
    expect(within(ribbon).queryByTestId("workspace-vm-stop")).toBeNull();
  });

  it("rolls the chevron back and reports a refused move", async () => {
    const patches = installWorkspace({
      rejectPatch: { status: 403, message: "Only the owner moves the phase" },
    });
    renderWorkspace();

    await screen.findByTestId("phase-chevrons");
    await userEvent.click(screen.getByTestId("phase-chevron-se"));
    await waitFor(() => expect(patches).toEqual([{ phase: "se" }]));

    const alert = await screen.findByTestId("phase-chevrons-error");
    expect(alert).toHaveAttribute("role", "alert");
    expect(alert).toHaveTextContent("Could not move the phase");
    expect(alert).toHaveTextContent("Only the owner moves the phase");
    await waitFor(() =>
      expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
        "data-phase",
        "be",
      ),
    );
    expect(screen.getByTestId("phase-chevron-be")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  // A move started on another page (the Home chip) is filed under the
  // workspace's shared mutation key: the chevrons here show it pending, settle
  // with it, and report its refusal — and refuse a second write meanwhile.
  it("shows a move started from Home as pending and settles with it", async () => {
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const patches = installWorkspace({ holdPatch: held });
    const queryClient = startMoveElsewhere("se");
    renderWorkspace(undefined, queryClient);

    const chevrons = await screen.findByTestId("phase-chevrons");
    expect(screen.getByTestId("phase-chevron-ie")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    // A second selection while the Home move is pending is ignored.
    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    expect(patches).toEqual([{ phase: "se" }]);

    release();
    await waitFor(() => expect(chevrons).toHaveAttribute("data-phase", "se"));
    await waitFor(() =>
      expect(screen.getByTestId("phase-chevron-ie")).not.toHaveAttribute(
        "aria-disabled",
      ),
    );
    expect(screen.queryByTestId("phase-chevrons-error")).toBeNull();
    expect(patches).toEqual([{ phase: "se" }]);

    // Settled: the owner's next move goes through.
    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    await waitFor(() =>
      expect(patches).toEqual([{ phase: "se" }, { phase: "ie" }]),
    );
  });

  it("reports a refused move started from Home and keeps the durable phase", async () => {
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const patches = installWorkspace({
      holdPatch: held,
      rejectPatch: { status: 403, message: "Only the owner moves the phase" },
    });
    const queryClient = startMoveElsewhere("se");
    renderWorkspace(undefined, queryClient);

    await screen.findByTestId("phase-chevrons");
    expect(screen.getByTestId("phase-chevron-se")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.queryByTestId("phase-chevrons-error")).toBeNull();
    release();
    const alert = await screen.findByTestId("phase-chevrons-error");
    expect(alert).toHaveTextContent("Only the owner moves the phase");
    expect(patches).toEqual([{ phase: "se" }]);
    await waitFor(() =>
      expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
        "data-phase",
        "be",
      ),
    );
    expect(screen.getByTestId("phase-chevron-se")).not.toHaveAttribute(
      "aria-disabled",
    );
  });

  // Workbench landing (g5.workbench.landing): the timeline is the first,
  // pinned center tab and the surface the workspace opens on.
  it("lands on the workspace landing above Activity", async () => {
    installWorkspace({ phase: "se" });
    renderWorkspace();

    const header = await screen.findByTestId("workspace-landing");
    expect(screen.getByTestId("workspace-activity-panel")).toBeVisible();
    // The phase lives in the chevrons only (g5.workbench.landing).
    expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
      "data-phase",
      "se",
    );
    expect(header).not.toHaveTextContent("System Engineering");
    // Summary line, then the task and implementation sections.
    const summary = within(header).getByTestId("landing-summary");
    const task = within(header).getByTestId("landing-task");
    const implementation = within(header).getByTestId("landing-implementation");
    expect(
      summary.compareDocumentPosition(task) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      task.compareDocumentPosition(implementation) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // Without a primary issue the native task stands alone: no status and
    // no invented issue link; its body is the workspace description.
    await waitFor(() =>
      expect(
        within(task).getByTestId("landing-task-preview"),
      ).toBeInTheDocument(),
    );
    expect(within(task).queryByTestId("landing-task-status")).toBeNull();
    expect(within(task).queryByTestId("landing-task-github")).toBeNull();
    // The feed carries the durable workbench entries; the served phase move
    // is never shown (2026-10-15).
    expect(
      await screen.findByTestId("activity-entry-pull-request:pr-evt-1"),
    ).toHaveTextContent("Pull request opened: Task graph rework");
    expect(screen.queryByTestId("activity-entry-phase:phase-evt-1")).toBeNull();
    // The filter pills sit below the landing; only All is initially pressed.
    const filters = screen.getByTestId("activity-filters");
    expect(
      header.compareDocumentPosition(filters) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen
        .getAllByTestId(/^activity-filter-[a-z_]+$/)
        .map((box) => box.getAttribute("data-testid")),
    ).toEqual([
      "activity-filter-all",
      "activity-filter-issue_discussion",
      "activity-filter-conversations",
      "activity-filter-tasks",
      "activity-filter-commits",
      "activity-filter-approvals",
      "activity-filter-pull_requests",
      "activity-filter-guided_steps",
    ]);
    for (const pill of screen.getAllByTestId(/^activity-filter-[a-z_]+$/)) {
      expect(pill).toHaveAttribute(
        "aria-pressed",
        pill.getAttribute("data-testid") === "activity-filter-all"
          ? "true"
          : "false",
      );
    }
    // Chat left the center: the Activity tab is selected, first and pinned.
    const tabs = screen.getAllByTestId("center-content-tab");
    expect(tabs[0]).toHaveTextContent("Workspace");
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(
      within(tabs[0].parentElement as HTMLElement).queryByTestId(
        "center-tab-close",
      ),
    ).toBeNull();
    expect(
      screen.queryByRole("tab", { name: /Chat/, selected: true }),
    ).toBeNull();
  });

  it.each([false, true])(
    "retains the Approvals URL after reload (delayed identity: %s)",
    async (delayIdentity) => {
      installWorkspace({ ownerUserId: "other-owner" });
      localStorage.setItem(
        "g5:tabs:task-graph-rework",
        JSON.stringify({
          tabs: [{ type: "workspace_approvals", name: "Approvals" }],
          activeTabId: "workspace-approvals",
        }),
      );
      let release!: () => void;
      const held = new Promise<void>((resolve) => {
        release = resolve;
      });
      server.use(
        http.get("/api/v1/workspaces/:id/review", async () => {
          if (delayIdentity) await held;
          return HttpResponse.json({
            data: { asSource: null, asReview: null },
          });
        }),
      );
      function ReloadSurface() {
        const location = useLocation();
        return (
          <>
            <div data-testid="reload-route">{location.search}</div>
            <WorkspacePage />
          </>
        );
      }
      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<ReloadSurface />} />
        </Routes>,
        {
          route: "/org-g5labs/task-graph-rework?view=approvals",
          user: createMockUser(),
        },
      );
      await screen.findByTestId("phase-chevrons");
      await act(async () => release());
      await screen.findByRole("heading", {
        name: /^Approvals Required to merge into /,
      });
      expect(screen.getByTestId("reload-route")).toHaveTextContent(
        "view=approvals",
      );
    },
  );

  it.each([false, true])(
    "retains the new Admin deep link while dismissing Chat (restored tab: %s)",
    async (restored) => {
      installWorkspace({});
      if (restored)
        localStorage.setItem(
          "g5:tabs:task-graph-rework",
          JSON.stringify({
            tabs: [{ type: "org_admin", name: "Admin" }],
            activeTabId: "org-admin",
          }),
        );
      function AdminRoute() {
        const location = useLocation();
        return (
          <>
            <div data-testid="admin-route">{location.search}</div>
            <WorkspacePage />
          </>
        );
      }
      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<AdminRoute />} />
        </Routes>,
        {
          route: "/org-g5labs/task-graph-rework?view=admin",
          user: createMockUser(),
        },
      );
      const tab = await screen.findByRole("tab", { name: /Admin/ });
      await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
      expect(screen.getByTestId("admin-route")).toHaveTextContent("view=admin");
      expect(await screen.findByTestId("org-admin-page-content")).toBeVisible();
      expect(
        screen.queryByRole("tab", { name: /Chat/, selected: true }),
      ).toBeNull();
    },
  );

  it("retains a restored Terminal deep link while opening the alignment workspace", async () => {
    installWorkspace({});
    localStorage.setItem(
      "g5:tabs:task-graph-rework",
      JSON.stringify({
        tabs: [{ type: "workspace_terminal", name: "Terminal" }],
        activeTabId: "workspace-terminal",
      }),
    );
    function TerminalRoute() {
      const location = useLocation();
      return (
        <>
          <div data-testid="terminal-route">{location.search}</div>
          <WorkspacePage />
        </>
      );
    }
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<TerminalRoute />} />
      </Routes>,
      {
        route: "/org-g5labs/task-graph-rework?view=terminal",
        user: createMockUser(),
      },
    );
    const tab = await screen.findByRole("tab", { name: /Terminal/ });
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("terminal-route")).toHaveTextContent(
      "view=terminal",
    );
    expect(
      screen.queryByRole("tab", { name: /Chat/, selected: true }),
    ).toBeNull();
  });

  it("opens the rich ontology comparison after workspace identity loads", async () => {
    installWorkspace({});
    renderWorkspace("/org-g5labs/test-workspace?view=ontology");
    const tab = await screen.findByRole("tab", { name: /Diff Graph/ });
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
  });

  it("lets a ?view= deep link win over the timeline landing", async () => {
    installWorkspace({});
    renderWorkspace("/org-g5labs/test-workspace?view=details");

    await screen.findByTestId("phase-chevrons");
    const details = await screen.findByRole("tab", {
      name: /Workspace Details/,
    });
    await waitFor(() =>
      expect(details).toHaveAttribute("aria-selected", "true"),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
    // The timeline stays the first, pinned tab.
    const tabs = screen.getAllByTestId("center-content-tab");
    expect(tabs[0]).toHaveTextContent("Workspace");
  });

  it("lands a review copy on its Review tab with Activity pinned second", async () => {
    installWorkspace({});
    server.use(
      http.get("/api/v1/workspaces/:id/review", () =>
        HttpResponse.json({
          data: {
            asSource: null,
            asReview: {
              forkId: "fork-1",
              orgId: "org-g5labs",
              sourceWorkspaceId: "ws-source-1",
              sourceWorkspaceName: "kaihui-test3",
              reviewWorkspaceId: "base",
              reviewWorkspaceName: "kaihui-test3-review-alex-chen",
              reviewerUserId: "user-alex",
              reviewerDisplayName: "Alex Chen",
              vmStatus: "stopped",
              vmStartedAt: null,
              vmLastActivityAt: null,
              lastFlushedAt: null,
              createdAt: "2026-09-22T16:10:00.000Z",
            },
            openSuggestions: 0,
          },
        }),
      ),
    );
    renderWorkspace();

    await screen.findByTestId("phase-chevrons");
    const review = await screen.findByRole("tab", {
      name: /Review of kaihui-test3/,
    });
    await waitFor(() =>
      expect(review).toHaveAttribute("aria-selected", "true"),
    );
    await waitFor(() => {
      const tabs = screen.getAllByTestId("center-content-tab");
      expect(tabs.map((tab) => tab.textContent)).toEqual([
        "Review of kaihui-test3",
        "Workspace",
      ]);
    });
    const tabs = screen.getAllByTestId("center-content-tab");
    expect(tabs[1]).toHaveAttribute("aria-selected", "false");
    // Both pinned: neither offers a close control.
    for (const tab of tabs) {
      expect(
        within(tab.parentElement as HTMLElement).queryByTestId(
          "center-tab-close",
        ),
      ).toBeNull();
    }
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
  });

  it("lets a ?session= deep link keep the chat in the center", async () => {
    installWorkspace({});
    renderWorkspace("/org-g5labs/test-workspace?session=chat-tgr-1");

    await screen.findByTestId("chat-phase-pill");
    expect(screen.getByTestId("right-panel")).toHaveAttribute(
      "data-chat-placement",
      "center",
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
  });
});

describe("WorkspacePage — Activity link destinations", () => {
  const workspacePath = "/g5labs/test-workspace";
  const chatEntry = MOCK_WORKSPACE_ACTIVITY.entries.find(
    (entry) => entry.kind === "chat",
  )!;
  const chatMessage = "What's the best approach for the DAG scheduler?";

  function ActivityRouteProbe() {
    const { pathname, search, hash } = useLocation();
    return <div data-testid="route-probe">{`${pathname}${search}${hash}`}</div>;
  }

  function serveActivity(
    entries: ActivityEntryDTO[],
    overrides: Partial<WorkspaceActivityDTO> = {},
  ) {
    server.use(
      http.get("/api/v1/workspaces/:id/activity", () =>
        HttpResponse.json({
          data: { ...MOCK_WORKSPACE_ACTIVITY, entries, ...overrides },
        }),
      ),
      http.get("/api/v1/workspaces/:id/activity/current", () =>
        HttpResponse.json({ data: { entries: [] } }),
      ),
    );
  }

  function renderActivity(route = workspacePath) {
    const page = <WorkspacePage />;
    return renderApp(
      <>
        <ActivityRouteProbe />
        <Routes>
          <Route path="/:org/issues/g5/:number" element={<IssuePage />} />
          <Route path="/:org/:workspace/task/:taskId" element={page} />
          <Route path="/:org/:workspace/*" element={page} />
        </Routes>
      </>,
      { route, user: createMockUser() },
    );
  }

  const entryRow = (key: string) =>
    screen.findByTestId(`activity-entry-${key}`);
  const chatPanel = () => screen.getByTestId("center-chat-panel");

  async function expectChat() {
    await waitFor(() => expect(chatPanel()).toBeVisible());
    await waitFor(() => expect(chatPanel()).toHaveTextContent(chatMessage));
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      `${workspacePath}?session=chat-tgr-1`,
    );
  }

  beforeEach(() => {
    installWorkspace({});
    installWorkbench();
    serveActivity([chatEntry]);
  });

  it("opens the selected conversation and restores its content after reload", async () => {
    const rendered = renderActivity();
    const row = await entryRow(chatEntry.key);
    await userEvent.click(
      within(row).getByRole("link", { name: "Implementation chat" }),
    );
    await expectChat();
    const route = screen.getByTestId("route-probe").textContent!;
    rendered.unmount();
    renderActivity(route);
    await expectChat();
  });

  it("opens a Current conversation's messages instead of the default session", async () => {
    const current: ActivityEntryDTO = {
      ...chatEntry,
      kind: "chat",
      sessionId: "chat-tgr-2",
      title: "Scheduler implementation",
      messageCount: 2,
      lastActivityAt: "2026-10-25T10:00:00Z",
      key: "chat:chat-tgr-2",
      taskStatus: "running",
    };
    server.use(
      http.get("/api/v1/workspaces/:id/activity/current", () =>
        HttpResponse.json({ data: { entries: [current] } }),
      ),
    );
    renderActivity();
    await userEvent.click(
      within(await entryRow(current.key)).getByRole("link", {
        name: current.title!,
      }),
    );
    await waitFor(() => expect(chatPanel()).toBeVisible());
    await waitFor(() =>
      expect(chatPanel()).toHaveTextContent(
        "I'll start by reading the existing scheduler module",
      ),
    );
    expect(chatPanel()).not.toHaveTextContent(chatMessage);
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      `${workspacePath}?session=chat-tgr-2`,
    );
  });

  it.each(["Workspace tab", "Overview control"])(
    "reopens the same conversation after returning through the %s",
    async (returnVia) => {
      if (returnVia === "Overview control")
        seedUserSettings({ showTabBar: false });
      renderActivity();
      const user = userEvent.setup();
      await user.click(
        within(await entryRow(chatEntry.key)).getByRole("link", {
          name: "Implementation chat",
        }),
      );
      await expectChat();

      if (returnVia === "Workspace tab") {
        await user.click(screen.getByRole("tab", { name: "Workspace" }));
      } else {
        await user.click(screen.getByTestId("ribbon-overview"));
      }

      expect(await entryRow(chatEntry.key)).toBeVisible();
      expect(chatPanel()).not.toBeVisible();
      await user.click(
        within(await entryRow(chatEntry.key)).getByRole("link", {
          name: "Implementation chat",
        }),
      );
      await expectChat();
    },
  );

  it.each(["command", "preview"] as const)(
    "opens a %s task's changes with the correct task identity",
    async (taskKind) => {
      const task = createMockTask({
        id: `timeline-${taskKind}`,
        workspaceId: RESOLVED_WORKSPACE_ID,
        status: "succeeded",
        name: `Timeline ${taskKind}`,
      });
      serveActivity([
        {
          key: `task:${task.id}`,
          at: "2026-10-25T10:00:00Z",
          source: "workspace",
          kind: "task",
          taskId: task.id,
          name: task.name,
          status: task.status,
          taskKind,
          reason: null,
        },
      ]);
      server.use(
        http.get("/api/v1/workspaces/:id/tasks", () =>
          HttpResponse.json({
            data: { tasks: [task], cursor: null, hasMore: false },
          }),
        ),
        http.get(`/api/v1/tasks/${task.id}`, () =>
          HttpResponse.json({ data: task }),
        ),
      );
      renderActivity();
      await userEvent.click(
        within(await entryRow(`task:${task.id}`)).getByRole("link", {
          name: `${task.name} succeeded`,
        }),
      );
      const changes = await screen.findByTestId("workspace-changes-view");
      await waitFor(() =>
        expect(
          within(changes).getByRole("heading", { name: "Changes" }),
        ).toBeVisible(),
      );
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        `${workspacePath}/task/${task.id}`,
      );
    },
  );

  it("opens the exact repository and commit, and restores that diff after reload", async () => {
    const commit: Extract<ActivityEntryDTO, { kind: "commit" }> = {
      key: "commit:timeline",
      at: "2026-10-25T10:00:00Z",
      source: "workspace",
      kind: "commit",
      repoId: "repo-g5-client",
      repoName: "g5-client",
      branchName: "main",
      sha: "abcdef1234567890",
      shortSha: "abcdef1",
      summary: "Timeline navigation fix",
      authorName: "Ada",
    };
    const diff: CommitDiffResponse = {
      commit: {
        sha: commit.sha,
        shortSha: commit.shortSha,
        summary: commit.summary,
        authorName: commit.authorName,
        committedAt: commit.at,
        parentCount: 0,
      },
      comparisonBase: { kind: "emptyTree" },
      firstParentDiff: false,
      files: [],
      totalAdditions: 0,
      totalDeletions: 0,
    };
    serveActivity([commit]);
    server.use(
      http.get(
        `/api/v1/workspaces/:id/repos/${commit.repoId}/commits/${commit.sha}/diff`,
        () => HttpResponse.json({ data: diff }),
      ),
    );
    const rendered = renderActivity();
    await userEvent.click(
      within(await entryRow(commit.key)).getByRole("button", {
        name: commit.summary,
      }),
    );
    const shown = await screen.findByTestId("workspace-commit-diff-view");
    expect(shown).toBeVisible();
    expect(shown).toHaveTextContent(commit.summary);
    expect(shown).toHaveTextContent(commit.shortSha);
    const route = `${workspacePath}/commit/${commit.repoId}/${commit.sha}`;
    expect(screen.getByTestId("route-probe")).toHaveTextContent(route);

    rendered.unmount();
    renderActivity(route);
    const restored = await screen.findByTestId("workspace-commit-diff-view");
    expect(restored).toBeVisible();
    expect(restored).toHaveTextContent(commit.summary);
    expect(restored).toHaveTextContent(commit.shortSha);
  });

  it.each(["approval_decision", "approval_requested"] as const)(
    "opens the shared Approvals destination from an %s",
    async (kind) => {
      seedPhasedApprovals();
      const shared = {
        key: "approval:timeline",
        at: "2026-10-25T10:00:00Z",
        source: "workspace" as const,
        phase: "be" as const,
        definitionName: "Sign-off",
        actorName: "Alex Chen",
      };
      const entry: ActivityEntryDTO =
        kind === "approval_decision"
          ? {
              ...shared,
              kind,
              outcome: "approved",
              actorType: "human",
              comment: null,
            }
          : {
              ...shared,
              kind,
              requestId: "request-timeline",
              reviewerName: "Maya Chen",
              cancelled: false,
              note: null,
            };
      serveActivity([entry]);
      renderActivity();
      await userEvent.click(
        within(await entryRow(entry.key)).getByRole("link"),
      );
      const content = screen.getByTestId("center-content-panel");
      const headings = await within(content).findAllByRole("heading", {
        name: /Approvals Required to merge into/,
      });
      expect(headings[0]).toBeVisible();
      expect(content).toBeVisible();
      expect(content).toHaveTextContent("Sign-off");
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        `${workspacePath}?view=approvals`,
      );
    },
  );

  it("opens the internal Pull Requests page from an unpublished PR event", async () => {
    serveActivity(MOCK_WORKSPACE_ACTIVITY.entries);
    renderActivity();
    await userEvent.click(
      within(await entryRow("pull-request:pr-evt-2")).getByRole("button", {
        name: "Merge blocked: Task graph rework",
      }),
    );
    expect(
      await screen.findByRole("heading", { name: "Pull Requests" }),
    ).toBeVisible();
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      `${workspacePath}?view=pull-requests`,
    );
  });

  it.each(["workspace_comment", "workspace_reply"] as const)(
    "opens the canonical discussion from a %s, even when its parent is outside the loaded history",
    async (kind) => {
      const parent: ReviewComment = {
        id: "timeline-parent",
        workspaceId: RESOLVED_WORKSPACE_ID,
        authorId: "user-maya",
        authorName: "Maya Chen",
        scope: "workspace",
        content: "Keep the task's original scope.",
        createdAt: "2026-10-24T10:00:00Z",
        resolved: false,
      };
      const reply = "The original scope is preserved.";
      const entry: ActivityEntryDTO =
        kind === "workspace_comment"
          ? {
              key: "discussion:timeline",
              at: parent.createdAt,
              source: "workspace",
              kind,
              comment: parent,
            }
          : {
              key: "discussion:timeline",
              at: "2026-10-25T10:00:00Z",
              source: "workspace",
              kind,
              commentId: parent.id,
              content: reply,
              actorName: "Alex Chen",
              phase: null,
            };
      serveActivity([entry]);
      server.use(
        http.get("/api/v1/workspaces/:id/comments", () =>
          HttpResponse.json({
            data: [
              {
                ...parent,
                id: "different-parent",
                content: "A different discussion.",
              },
              parent,
            ],
          }),
        ),
        http.get(`/api/v1/comments/${parent.id}/replies`, () =>
          HttpResponse.json({
            data: [
              { id: "timeline-reply", authorName: "Alex Chen", content: reply },
            ],
          }),
        ),
      );
      const { queryClient } = renderActivity();
      queryClient.setQueryData(
        ["workspace-review-replies", parent.id],
        [{ id: "timeline-reply", authorName: "Alex Chen", content: reply }],
      );
      await userEvent.click(
        within(await entryRow(entry.key)).getByRole("button", {
          name:
            kind === "workspace_comment"
              ? "Maya Chen commented"
              : "Alex Chen replied",
        }),
      );
      const reader = await screen.findByTestId("activity-thread-reader");
      expect(reader).toBeVisible();
      expect(reader).toHaveTextContent(parent.content);
      expect(reader).toHaveTextContent(reply);
      expect(reader).not.toHaveTextContent("A different discussion.");
    },
  );

  it.each(["issue", "comment"] as const)(
    "opens the native issue page from an %s link and retains its comment anchor",
    async (from) => {
      serveNativeIssuesFeature();
      const issue = makeNativeIssueDetail();
      const comment = issue.comments[0];
      const url = from === "issue" ? issue.canonicalUrl : comment.url;
      const entry: ActivityEntryDTO =
        from === "issue"
          ? {
              key: "native:timeline",
              kind: "issue_opened",
              source: "g5",
              at: issue.createdAt,
              author: issue.author,
              title: issue.title,
              body: issue.body,
              url,
            }
          : {
              key: "native:timeline",
              kind: "issue_comment",
              source: "g5",
              at: comment.createdAt,
              author: comment.author,
              commentId: comment.id,
              body: comment.body,
              updatedAt: comment.updatedAt,
              url,
            };
      serveActivity([entry]);
      server.use(
        http.get("/api/v1/orgs/org-g5labs/issues/g5/12", () =>
          HttpResponse.json({ data: issue }),
        ),
      );
      renderActivity();
      await userEvent.click(
        within(await entryRow(entry.key)).getByRole("link"),
      );
      const detail = await screen.findByTestId("external-issue-detail");
      expect(detail).toBeVisible();
      expect(detail).toHaveTextContent(issue.title);
      expect(detail).toHaveTextContent(comment.body);
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        `/g5labs/issues/g5/12${from === "comment" ? `#comment-${comment.id}` : ""}`,
      );
    },
  );
});

// Workbench Phase 2 (g5.workbench.actions, g5.workbench.persona,
// g5.workbench.mock): the ribbon dispatches templated chats and existing
// surfaces; the chat shows its persona and follows the view.
// workspaces g5.workspace.ui.chat-corner-button: the same corner button under
// the alignment layout, where Chat is a center tab; the button is absent while
// that tab is selected, the open moves Chat to the floating panel at the right
// edge and the tab stops being selected.
describe("WorkspacePage — corner Chat button under the alignment layout", () => {
  it("opens Chat at the right edge below the workbench header and leaves the center to the content", async () => {
    installWorkspace({ phase: "se" });
    const user = userEvent.setup();
    // jsdom lays nothing out; give the chrome rows heights so the placement
    // can be checked: tab bar 0-39, workbench header (chevrons + ribbon)
    // 39-166, as a browser renders them.
    const rects = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockImplementation(function (this: HTMLElement) {
        const id = this.getAttribute("data-testid");
        const bottom =
          id === "workbench-header" ? 166 : id === "workspace-tab-bar" ? 39 : 0;
        return {
          x: 0,
          y: 0,
          top: 0,
          left: 0,
          right: 0,
          bottom,
          width: 0,
          height: bottom,
          toJSON: () => ({}),
        } as DOMRect;
      });
    try {
      renderWorkspace("/org-g5labs/test-workspace?session=chat-tgr-1");

      expect(await screen.findByTestId("chat-phase-pill")).toBeInTheDocument();
      // Chat holds the center, so the button is away; the Activity tab
      // brings it back.
      expect(screen.queryByTestId("chat-corner-button")).toBeNull();
      await user.click(screen.getAllByTestId("center-content-tab")[0]);
      const button = await screen.findByTestId("chat-corner-button");
      await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
      await user.click(button);

      const dock = await screen.findByTestId("floating-panel-dock");
      const panel = dock.parentElement!.parentElement as HTMLElement;
      // Under the ribbon, so its Chat toggle and workspace controls stay
      // reachable, and down to the bottom.
      expect(panel.style.top).toBe("166px");
      expect(panel.style.height).toBe(`${window.innerHeight - 166}px`);
    } finally {
      rects.mockRestore();
    }
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-tab")).toHaveAttribute(
        "aria-selected",
        "false",
      ),
    );

    // A plain click: the Dock control sits in the drag title bar, and jsdom
    // has no pointer capture.
    fireEvent.click(screen.getByTestId("floating-panel-dock"));
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-tab")).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    // Chat is the center surface again, where the button has no job.
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
  });
});

describe("WorkspacePage — alignment ribbon actions (g5.workbench.actions)", () => {
  it("Build mock's dialog seeds a fresh composer with the expanded prompt, and the send starts a BE session under the persona with the revision loaded at send time", async () => {
    installWorkspace({});
    const { starts, resolves } = installWorkbench();
    renderWorkspace();

    // Build mock is one dialog (g5.workbench.mock): the ribbon entry opens
    // it and its primary action seeds the hidden `generate-mock` template,
    // which the ribbon never draws.
    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    expect(
      screen.queryByTestId("ribbon-action-generate-mock"),
    ).not.toBeInTheDocument();
    await userEvent.click(build);
    const dialog = await screen.findByRole("dialog", { name: "Build mock" });
    expect(within(dialog).getByTestId("build-mock-empty")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByTestId("build-mock-submit"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Seeded, never sent (g5.workbench.chat-actions): the fresh composer
    // holds the prompt expanded with its phase context, and Send pulses.
    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "generate-mock")!,
      "be",
      { kind: "phase" },
      "",
    );
    expect(expected).toContain(
      "Context: the workspace is in the Business Engineering phase.",
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue(expected));
    expect(starts).toHaveLength(0);
    const send = screen.getByTestId("chat-panel-no-session-send");
    await waitFor(() => expect(send).toBeEnabled());
    expect(send).toHaveAttribute("data-pulse", "true");
    await userEvent.click(send);

    await waitFor(() => expect(starts).toHaveLength(1));
    // The launch resolves the user's explicit defaults: inheriting would
    // need a workspace-configured permission mode a fresh workspace lacks.
    expect(resolves.length).toBeGreaterThan(0);
    for (const body of resolves) {
      expect(body).toMatchObject({
        preferences: { permissionMode: { source: "explicit" } },
      });
    }
    // The browser expanded the template: the text as sent is the request,
    // under the action's persona and the clicked revision; no action id or
    // context travel.
    expect(starts[0]).toMatchObject({
      persona: "be",
      phase: "be",
      configRevision: WORKBENCH_REVISION,
      content: expected,
      scopeType: "workspace",
    });
    expect(starts[0]).not.toHaveProperty("actionId");
    expect(starts[0]).not.toHaveProperty("workbenchContext");
    expect(String(starts[0].requestKey)).toMatch(/^workbench-/);
    // The new session opens in the chat with its persona pill.
    const pill = await screen.findByTestId("chat-phase-pill");
    await waitFor(() => expect(pill).toHaveAttribute("data-source", "persona"));
    expect(pill).toHaveTextContent("BE");
    // Same persona as the workbench phase: nothing to follow.
    expect(screen.queryByTestId("chat-follow-view")).toBeNull();
  });

  it("a chat action seeded into the open chat that already carries its persona goes into that chat, not a fresh one", async () => {
    installWorkspace({});
    const { starts } = installWorkbench({ sessionPersona: "be" });
    renderWorkspace("/org-g5labs/test-workspace?session=chat-tgr-1");
    const pill = await screen.findByTestId("chat-phase-pill");
    await waitFor(() => expect(pill).toHaveAttribute("data-source", "persona"));

    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    await userEvent.click(build);
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Build mock" }),
      ).getByTestId("build-mock-submit"),
    );

    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "generate-mock")!,
      "be",
      { kind: "phase" },
      "",
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(expected),
    );
    // Still the open chat: its own Send pulses and nothing started.
    expect(screen.getByTestId("chat-phase-pill")).toHaveTextContent("BE");
    expect(screen.getByTestId("chat-panel-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );
    expect(starts).toHaveLength(0);
  });

  it("a chat action keeps the clicked phase even when its persona is overridden", async () => {
    installWorkspace({ phase: "se" });
    const { starts } = installWorkbench();
    server.use(
      http.get("/api/v1/workspaces/:id/workbench", () =>
        HttpResponse.json({
          data: {
            revision: WORKBENCH_REVISION,
            actions: {
              ...WORKBENCH_ACTIONS,
              se: [
                {
                  name: "Analysis",
                  actions: [
                    {
                      id: "analyze-test-plan",
                      label: "Test plan",
                      kind: "chat",
                      persona: "be",
                      prompt: "SE's own test plan prompt",
                    },
                  ],
                },
              ],
            },
            personas: {
              be: { source: "builtin", text: "be" },
              se: { source: "builtin", text: "se" },
              ie: { source: "builtin", text: "ie" },
            },
            config: { status: "committed" },
          },
        }),
      ),
    );
    renderWorkspace();
    const plan = await screen.findByTestId("ribbon-action-analyze-test-plan");
    await waitFor(() => expect(plan).toBeEnabled());
    await userEvent.click(plan);
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue("SE's own test plan prompt"));
    expect(starts).toHaveLength(0);
    const send = screen.getByTestId("chat-panel-no-session-send");
    await waitFor(() => expect(send).toBeEnabled());
    await userEvent.click(send);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      persona: "be",
      phase: "se",
      content: "SE's own test plan prompt",
      configRevision: WORKBENCH_REVISION,
    });
    expect(starts[0]).not.toHaveProperty("actionId");
  });

  // The seed was made at one revision; a docs commit (an answered question)
  // moved it before the send. The send pins the version loaded now, not
  // the one of the click (issue #1142, g5.workbench.chat-actions).
  it("a seeded send carries the workbench revision loaded at send time, not the click's", async () => {
    installWorkspace({});
    const { starts } = installWorkbench();
    const queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false },
        mutations: { retry: false },
      },
    });
    renderWorkspace("/org-g5labs/test-workspace", queryClient);

    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    await userEvent.click(build);
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Build mock" }),
      ).getByTestId("build-mock-submit"),
    );
    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "generate-mock")!,
      "be",
      { kind: "phase" },
      "",
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue(expected));

    // The docs head moved: the next configuration poll serves a new revision.
    const moved = "mock-head-2.builtin0000";
    server.use(
      http.get("/api/v1/workspaces/:id/workbench", () =>
        HttpResponse.json({
          data: {
            revision: moved,
            actions: WORKBENCH_ACTIONS,
            personas: {
              be: { source: "builtin", text: "be" },
              se: { source: "builtin", text: "se" },
              ie: { source: "builtin", text: "ie" },
            },
            config: { status: "builtin" },
          },
        }),
      ),
    );
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: workbenchConfigKey(RESOLVED_WORKSPACE_ID),
      });
    });
    await waitFor(() =>
      expect(
        queryClient.getQueryData<{ revision: string }>(
          workbenchConfigKey(RESOLVED_WORKSPACE_ID),
        )?.revision,
      ).toBe(moved),
    );
    // The seed survived the poll: same prompt, Send still pulsing.
    expect(input).toHaveValue(expected);
    const send = screen.getByTestId("chat-panel-no-session-send");
    expect(send).toHaveAttribute("data-pulse", "true");
    await userEvent.click(send);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      persona: "be",
      phase: "be",
      configRevision: moved,
      content: expected,
    });
  });

  // A stale pin is refused once (a commit landed inside the poll window);
  // the hook reloads the configuration, the draft and pulse stay, and the
  // next Send carries the reloaded revision and starts (issue #1142).
  it("a seeded send refused as stale keeps the draft, and the next Send starts under the reloaded revision", async () => {
    installWorkspace({});
    const { starts } = installWorkbench();
    const current = "mock-head-3.builtin0000";
    let served = WORKBENCH_REVISION;
    server.use(
      http.get("/api/v1/workspaces/:id/workbench", () =>
        HttpResponse.json({
          data: {
            revision: served,
            actions: WORKBENCH_ACTIONS,
            personas: {
              be: { source: "builtin", text: "be" },
              se: { source: "builtin", text: "se" },
              ie: { source: "builtin", text: "ie" },
            },
            config: { status: "builtin" },
          },
        }),
      ),
      http.post(
        "/api/v1/workspaces/:id/sessions/start",
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          starts.push(body);
          if (body.configRevision !== current) {
            // From now on the poll serves the current revision.
            served = current;
            return HttpResponse.json(
              {
                error: {
                  code: "CONFLICT",
                  message:
                    "The workbench configuration changed; reload it and try again",
                  details: {
                    code: "WORKBENCH_CONFIG_STALE",
                    requestedRevision: body.configRevision,
                    currentRevision: current,
                  },
                },
              },
              { status: 409 },
            );
          }
          const session = mockSession(`session-wb-${starts.length}`, "be");
          return HttpResponse.json(
            {
              data: {
                session,
                message: {
                  messageId: "in-1",
                  status: "queued",
                  createdAt: "2026-10-06T00:00:00Z",
                },
                assistantMessageId: "in-1",
                taskId: "task-wb",
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderWorkspace();

    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    await userEvent.click(build);
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Build mock" }),
      ).getByTestId("build-mock-submit"),
    );
    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "generate-mock")!,
      "be",
      { kind: "phase" },
      "",
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue(expected));
    const send = screen.getByTestId("chat-panel-no-session-send");
    await waitFor(() => expect(send).toBeEnabled());

    await userEvent.click(send);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({ configRevision: WORKBENCH_REVISION });
    // Refused: the toast names the retry, the draft and its pulse remain.
    await waitFor(() =>
      expect(
        screen
          .getAllByTestId("toast")
          .some((toast) =>
            toast.textContent?.includes(
              "The workbench configuration changed; try again",
            ),
          ),
      ).toBe(true),
    );
    expect(input).toHaveValue(expected);
    expect(screen.getByTestId("chat-panel-no-session-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );

    // The second Send carries the reloaded revision and starts the session.
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeEnabled(),
    );
    await userEvent.click(screen.getByTestId("chat-panel-no-session-send"));
    await waitFor(() => expect(starts).toHaveLength(2));
    expect(starts[1]).toMatchObject({
      persona: "be",
      phase: "be",
      configRevision: current,
      content: expected,
    });
    const pill = await screen.findByTestId("chat-phase-pill");
    await waitFor(() => expect(pill).toHaveAttribute("data-source", "persona"));
  });

  it("keeps Review meeting, Build mock and the chat actions open while the workspace is stopped (g5.chat.start-and-send, g5.workbench.meeting)", async () => {
    installWorkspace({});
    installWorkbench();
    server.use(
      http.get("/api/v1/workspaces/:id/vm", ({ params }) =>
        HttpResponse.json({
          data: createMockWorkspaceVm({
            workspaceId: String(params.id),
            ownerUserId: "user-alex",
            status: "stopped",
          }),
        }),
      ),
    );
    renderWorkspace();
    // Review meeting opens its dialog: a stored transcript only seeds the
    // extraction chat, which Start and send carries; only the upload path
    // needs the running VM.
    const meeting = await screen.findByTestId("ribbon-action-review-meeting");
    await waitFor(() => expect(meeting).toBeEnabled());
    // A chat action is greyed only with the composer's reason, and the
    // owner's stopped composer offers Start and send, so it stays open.
    const findGaps = screen.getByTestId("ribbon-action-open-questions");
    await waitFor(() => expect(findGaps).toBeEnabled());
    // The Build mock dialog's primary action follows the same rule.
    const build = screen.getByTestId("ribbon-action-build-mock");
    expect(build).toBeEnabled();
    await userEvent.click(build);
    const mockDialog = await screen.findByRole("dialog", {
      name: "Build mock",
    });
    const submit = within(mockDialog).getByTestId("build-mock-submit");
    await waitFor(() => expect(submit).toBeEnabled());
    await userEvent.click(within(mockDialog).getByTestId("build-mock-cancel"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // New doc stays available: only the document write needs the VM.
    expect(screen.getByTestId("ribbon-action-new-doc")).toBeEnabled();
  });

  it("offers Review meeting while the workspace is running", async () => {
    installWorkspace({});
    installWorkbench();
    renderWorkspace();
    const meeting = await screen.findByTestId("ribbon-action-review-meeting");
    await waitFor(() => expect(meeting).toBeEnabled());
  });

  it("offers no Analyze action in the built-in ribbon", async () => {
    installWorkspace({});
    installWorkbench();
    renderWorkspace();
    await screen.findByTestId("ribbon-action-review-meeting");
    expect(screen.queryByTestId("ribbon-action-analyze")).toBeNull();
    expect(screen.queryByTestId("ribbon-action-progress-overview")).toBeNull();
  });

  it("counts requirements by their server status before PR submission, with no phase meters (g5.workbench.landing.summary)", async () => {
    installWorkspace({ phase: "ie" });
    installWorkbench();
    seedPhasedApprovals(false);
    renderWorkspace();
    await screen.findByTestId("landing-summary-approvals");
    expect(screen.queryAllByTestId(/^phase-approval-meter-/)).toHaveLength(0);
    // Requests no longer need a PR, so the line reads the same either way.
    await waitFor(() =>
      expect(screen.getByTestId("landing-summary-approvals")).toHaveTextContent(
        "0 approved, 0 rejected, 1 pending",
      ),
    );
  });

  it("shows one workspace approval summary and refuses Merge while target requirements are pending", async () => {
    installWorkspace({ phase: "se" });
    installWorkbench();
    seedPhasedApprovals();
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/activity", () =>
        HttpResponse.json({
          data: {
            ...MOCK_WORKSPACE_ACTIVITY,
            entries: [
              ...MOCK_WORKSPACE_ACTIVITY.entries,
              {
                key: "approval:apr-be-1",
                at: "2026-09-01T10:00:00Z",
                source: "workspace",
                kind: "approval_decision",
                definitionName: "Sign-off",
                outcome: "approved",
                phase: "be",
                actorType: "human",
                actorName: "Alex Chen",
                comment: null,
              },
            ],
          },
        }),
      ),
    );
    renderWorkspace();

    const approvals = await screen.findByTestId("landing-summary-approvals");
    await waitFor(() =>
      expect(approvals).toHaveTextContent("0 approved, 0 rejected, 1 pending"),
    );
    // The summary is navigation to the one Approvals destination.
    expect(approvals).toHaveAttribute(
      "href",
      expect.stringContaining("?view=approvals"),
    );
    expect(
      within(screen.getByTestId("phase-chevrons")).queryByTestId(
        /^phase-approval-meter-/,
      ),
    ).toBeNull();
    // The BE decision is on the timeline, named by its phase.
    expect(
      await screen.findByTestId("activity-entry-approval:apr-be-1"),
    ).toHaveTextContent("Alex Chen approved Sign-off");

    // Merge to parent opens the ship surface, where the server's admission
    // disables shipping with its reason.
    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    const merge = await screen.findByTestId("ribbon-action-merge");
    await waitFor(() => expect(merge).toBeEnabled());
    await userEvent.click(merge);
    expect(
      await screen.findByRole("heading", { name: /Prepare to Ship/ }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("prepare-to-ship-ship-button")).toBeDisabled();
    expect(screen.getByTestId("prepare-to-ship-ship-note")).toHaveTextContent(
      /^Approval\(s\) required before merging$/,
    );
  });

  it("opens shared Approvals after the action icons and before workspace controls across phases", async () => {
    installWorkspace({ phase: "be" });
    installWorkbench();
    seedPhasedApprovals();
    renderWorkspace();
    const approvals = await screen.findByTestId("ribbon-workspace-approvals");
    expect(screen.queryByTestId("sidebar-approvals-tab")).toBeNull();
    const ribbon = screen.getByTestId("action-ribbon");
    const controls = screen.getByTestId("ribbon-trailing");
    expect(screen.getByTestId("ribbon-end-actions")).toContainElement(
      approvals,
    );
    expect(controls).not.toContainElement(approvals);
    const buttons = within(ribbon).getAllByRole("button");
    const firstControl = within(controls).getAllByRole("button")[0];
    expect(buttons[buttons.indexOf(approvals) + 1]).toBe(firstControl);
    expect(screen.queryByTestId("ribbon-action-approvals")).toBeNull();
    await userEvent.click(approvals);
    expect(
      await screen.findByTestId("workspace-approvals-outgoing"),
    ).toBeVisible();
    expect(screen.queryByTestId("workspace-approvals-phase-filter")).toBeNull();
    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    expect(screen.queryByTestId("ribbon-action-approval")).toBeNull();
    await userEvent.click(approvals);
    expect(
      await screen.findByTestId("workspace-approvals-outgoing"),
    ).toBeVisible();
  });

  it.each(
    (["owner", "non-owner"] as const).flatMap((role) =>
      (["be", "se", "ie"] as const).map((phase) => ({ role, phase })),
    ),
  )(
    "$role can read the landing and shared Approvals in $phase",
    async ({ role, phase }) => {
      const patches = installWorkspace({
        ownerUserId: role === "owner" ? "user-alex" : "user-somebody-else",
        phase,
      });
      installWorkbench();
      renderWorkspace("/org-g5labs/test-workspace?view=activity");
      const header = await screen.findByTestId("workspace-landing");
      expect(screen.getAllByTestId("center-content-tab")[0]).toHaveTextContent(
        "Workspace",
      );
      await userEvent.click(
        await within(header).findByTestId("workspace-read-task-description"),
      );
      const dialog = screen.getByRole("dialog", { name: "Workspace task" });
      expect(
        within(dialog).getByRole("region", { name: "Full task description" }),
      ).not.toBeEmptyDOMElement();
      expect(within(dialog).queryByRole("button", { name: "Done" })).toBeNull();
      await userEvent.keyboard("{Escape}");
      expect(
        screen.queryByRole("dialog", { name: "Workspace task" }),
      ).toBeNull();

      await userEvent.click(screen.getByTestId("ribbon-workspace-approvals"));
      expect(
        await screen.findByTestId("workspace-approvals-outgoing"),
      ).toBeVisible();
      expect(
        screen.queryByTestId("workspace-approvals-phase-filter"),
      ).toBeNull();
      expect(screen.queryByTestId("ribbon-action-approvals")).toBeNull();
      expect(screen.queryByTestId("ribbon-action-approval")).toBeNull();
      // The ribbon's Overview control opens the landing
      // (g5.workbench.landing).
      await userEvent.click(screen.getByTestId("ribbon-overview"));
      expect(await screen.findByTestId("workspace-landing")).toBeVisible();
      expect(screen.getByRole("heading", { name: "Activity" })).toBeVisible();
      if (role === "non-owner") {
        const next = screen.getByTestId(
          `phase-chevron-${phase === "be" ? "se" : "be"}`,
        );
        expect(next).toHaveAttribute("aria-disabled", "true");
        await userEvent.click(next);
      }
      expect(patches).toEqual([]);
    },
  );

  // g5.workbench.ship-actions: SE lands with Rebase (the source-only Rebase
  // Workspace dialog); IE releases with Create PR and Merge to parent (the
  // ship surface).
  it("Rebase opens the Rebase Workspace dialog; Create PR and Merge to parent open the ship surface, never the rebase confirm", async () => {
    installWorkspace({ phase: "se" });
    installWorkbench();
    // A forked workspace: its parent is the one rebase target.
    renderWorkspace("/org-g5labs/task-graph-rework");

    expect(screen.queryByTestId("ribbon-action-create-pr")).toBeNull();
    const rebase = await screen.findByTestId("ribbon-action-rebase");
    await waitFor(() => expect(rebase).toBeEnabled());
    expect(rebase).toHaveAttribute(
      "title",
      "Rebase this workspace onto a parent without advancing it",
    );
    await userEvent.click(rebase);
    expect(
      await screen.findByRole("heading", { name: /Rebase Workspace/ }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: /Prepare to Ship/ }),
    ).toBeNull();
    await userEvent.click(screen.getByTestId("workspace-merge-modal-close"));
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: /Rebase Workspace/ }),
      ).toBeNull(),
    );

    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    expect(screen.queryByTestId("ribbon-action-rebase")).toBeNull();
    const createPr = await screen.findByTestId("ribbon-action-create-pr");
    await waitFor(() => expect(createPr).toBeEnabled());
    expect(createPr).toHaveTextContent("Create PR");
    await userEvent.click(createPr);
    expect(
      await screen.findByRole("heading", { name: /Prepare to Ship/ }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("prepare-to-ship-close-button"));
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: /Prepare to Ship/ }),
      ).toBeNull(),
    );

    const merge = await screen.findByTestId("ribbon-action-merge");
    await waitFor(() => expect(merge).toBeEnabled());
    expect(merge).toHaveAttribute(
      "title",
      expect.stringMatching(/Merge the pull request into parent/),
    );
    await userEvent.click(merge);
    expect(
      await screen.findByRole("heading", { name: /Prepare to Ship/ }),
    ).toBeInTheDocument();
    // The rebase-only confirm dialog is not what Merge to parent opens.
    expect(screen.queryByTestId("direct-clean-merge-cancel")).toBeNull();
  });

  it("Code review and Re-run Verification seed a fresh composer with their prompt and header (g5.workbench.review-prompts)", async () => {
    installWorkspace({ phase: "se" });
    const { starts } = installWorkbench();
    // The organization's administrator has turned Re-run Verification on.
    server.use(
      http.get("/api/v1/orgs/:orgId/config", () =>
        HttpResponse.json({
          data: {
            scope: "org",
            settings: { [VERIFICATION_ACTION_ENABLED_SETTING]: true },
          },
        }),
      ),
    );
    renderWorkspace();

    const review = await screen.findByTestId("ribbon-action-code-review");
    await waitFor(() => expect(review).toBeEnabled());
    await userEvent.click(review);
    const input = await screen.findByTestId("chat-input");
    await waitFor(() =>
      expect(input).toHaveValue(reviewWorkspaceChatPrompt(1)),
    );
    const iterations = screen.getByTestId("composer-seed-review-iterations");
    expect(iterations).toHaveValue("1");
    await userEvent.selectOptions(iterations, "3");
    expect(screen.getByTestId("chat-input")).toHaveValue(
      reviewWorkspaceChatPrompt(3),
    );
    expect(screen.getByTestId("chat-panel-no-session-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );

    // Integration Engineering's Release group: Re-run Verification sits
    // before Create PR.
    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    const verify = await screen.findByTestId(
      "ribbon-action-rerun-verification",
    );
    const createPr = screen.getByTestId("ribbon-action-create-pr");
    expect(
      verify.compareDocumentPosition(createPr) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(verify).toHaveTextContent("Re-run Verification");
    await waitFor(() => expect(verify).toBeEnabled());
    await userEvent.click(verify);
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(
        verificationChatPrompt(),
      ),
    );
    expect(
      screen.getByTestId("composer-seed-verification-notice"),
    ).toHaveTextContent(VERIFICATION_NOTICE);
    // The fresh chat replaced the review seed and its option.
    expect(screen.queryByTestId("composer-seed-review-loop")).toBeNull();
    // Seeded, never sent.
    expect(starts).toHaveLength(0);
  });

  it("Re-run Verification is greyed with the reason until the organization turns it on (g5.workbench.review-prompts)", async () => {
    installWorkspace({ phase: "ie" });
    installWorkbench();
    // An organization that never touched the switch: off by default.
    const configReads: string[] = [];
    server.use(
      http.get("/api/v1/orgs/:orgId/config", ({ params }) => {
        configReads.push(String(params.orgId));
        return HttpResponse.json({ data: { scope: "org", settings: {} } });
      }),
    );
    renderWorkspace();

    await screen.findByTestId("ribbon-action-rerun-verification");
    // Code review, offered on the same terms, is available, so only the
    // switch greys Re-run Verification.
    await userEvent.click(screen.getByTestId("phase-chevron-se"));
    await waitFor(() =>
      expect(screen.getByTestId("ribbon-action-code-review")).toBeEnabled(),
    );
    await userEvent.click(screen.getByTestId("phase-chevron-ie"));
    const greyed = await screen.findByTestId(
      "ribbon-action-rerun-verification",
    );
    await waitFor(() => expect(configReads.length).toBeGreaterThan(0));
    expect(greyed).toBeDisabled();
    expect(greyed).toHaveAttribute("title", VERIFICATION_ACTION_OFF_TITLE);
    // Clicking a greyed action seeds nothing.
    await userEvent.click(greyed);
    expect(
      screen.queryByTestId("composer-seed-verification-notice"),
    ).toBeNull();
  });

  it("Get Approvals sits between Create PR and Merge to parent and seeds an IE chat that the person's send starts (g5.workbench.get-approvals)", async () => {
    installWorkspace({ phase: "ie" });
    const { starts } = installWorkbench();
    renderWorkspace();

    const getApprovals = await screen.findByTestId(
      "ribbon-action-get-approvals",
    );
    const createPr = screen.getByTestId("ribbon-action-create-pr");
    const merge = screen.getByTestId("ribbon-action-merge");
    expect(
      createPr.compareDocumentPosition(getApprovals) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      getApprovals.compareDocumentPosition(merge) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(getApprovals).toHaveTextContent("Get Approvals");
    await waitFor(() => expect(getApprovals).toBeEnabled());
    await userEvent.click(getApprovals);

    // Seeded, never sent: the prompt with its phase context, Send pulsing.
    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.ie, "get-approvals")!,
      "ie",
      { kind: "phase" },
      "",
    );
    expect(expected).toContain(
      "Context: the workspace is in the Integration Engineering phase.",
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue(expected));
    expect(starts).toHaveLength(0);
    const send = screen.getByTestId("chat-panel-no-session-send");
    await waitFor(() => expect(send).toBeEnabled());
    expect(send).toHaveAttribute("data-pulse", "true");

    // The person's send is the request, under the IE persona.
    await userEvent.click(send);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      persona: "ie",
      phase: "ie",
      configRevision: WORKBENCH_REVISION,
      content: expected,
    });
    expect(starts[0]).not.toHaveProperty("actionId");
  });

  it("Get Approvals is greyed with the chat reason for someone who cannot send (g5.workbench.get-approvals)", async () => {
    installWorkspace({ phase: "ie", ownerUserId: "another-user" });
    const { starts } = installWorkbench();
    renderWorkspace();

    const getApprovals = await screen.findByTestId(
      "ribbon-action-get-approvals",
    );
    await waitFor(() => expect(getApprovals).toBeDisabled());
    expect(getApprovals).toHaveAttribute(
      "title",
      "Chat needs a writable workspace you own",
    );
    await userEvent.click(getApprovals);
    // Clicking a greyed action seeds nothing and starts nothing.
    expect(screen.getByTestId("chat-input")).toHaveValue("");
    expect(starts).toHaveLength(0);
  });

  it("Build mock lists this workspace's stored mocks newest first and opens one in its reader tab; there is no Preview mock", async () => {
    installWorkspace({ detail: { docsFolderPath: "docs/base" } });
    const { starts } = installWorkbench();
    server.use(
      http.get("/api/v1/workspaces/:id/docs", () =>
        HttpResponse.json({
          data: [
            createMockDocNode({
              id: "mock-older",
              name: "Checkout mock",
              filePath: "docs/base/mocks/checkout.html",
              docType: "doc",
              contentUpdatedAt: "2026-10-28T09:00:00Z",
              updatedAt: "2026-10-28T09:00:00Z",
            }),
            createMockDocNode({
              id: "mock-newest",
              name: "Mock index",
              filePath: "docs/base/mocks/index.html",
              docType: "doc",
              contentUpdatedAt: "2026-10-30T09:00:00Z",
              updatedAt: "2026-10-30T09:00:00Z",
            }),
            createMockDocNode({
              id: "doc-requirement",
              name: "Requirement",
              filePath: "docs/base/requirement.md",
              contentUpdatedAt: "2026-10-31T09:00:00Z",
              updatedAt: "2026-10-31T09:00:00Z",
            }),
          ],
        }),
      ),
    );
    renderWorkspace();

    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    // Mocks are Project Docs: no Preview mock entry, no profile gate.
    expect(screen.queryByTestId("ribbon-action-preview-mock")).toBeNull();
    expect(screen.queryByTestId("ribbon-preview-preview-mock")).toBeNull();
    // Review meeting opens the transcript dialog (g5.workbench.meeting).
    expect(screen.getByTestId("ribbon-action-review-meeting")).toBeEnabled();

    await userEvent.click(build);
    const dialog = await screen.findByRole("dialog", { name: "Build mock" });
    const rows = within(
      await within(dialog).findByTestId("build-mock-list"),
    ).getAllByRole("button");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("index.html"),
      expect.stringContaining("checkout.html"),
    ]);
    await userEvent.click(
      within(dialog).getByTestId("build-mock-open-mock-older"),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(
      await screen.findByRole("tab", { name: /Checkout mock/ }),
    ).toBeInTheDocument();
    // Opening a mock seeds nothing and starts nothing.
    expect(
      (screen.queryByTestId("chat-input") as HTMLTextAreaElement | null)
        ?.value ?? "",
    ).toBe("");
    expect(starts).toHaveLength(0);
  });

  it("New doc and Upload file open the Create Doc modal in their modes", async () => {
    installWorkspace({});
    installWorkbench();
    renderWorkspace();

    await userEvent.click(await screen.findByTestId("ribbon-action-new-doc"));
    expect(await screen.findByRole("dialog")).toHaveTextContent("Create Doc");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    await userEvent.click(screen.getByTestId("ribbon-action-import"));
    await userEvent.click(screen.getByTestId("ribbon-action-upload-file"));
    expect(await screen.findByRole("dialog")).toHaveTextContent("Upload Docs");
  });

  // Workbench Phase 4 (g5.workbench.meeting): Review meeting opens the modal
  // in transcript mode; an uploaded transcript is stored and opens, and the
  // phase's hidden `extract-decisions` template starts over it with
  // the transcript's repository path as the current document.
  it("Review meeting uploads the transcript under meetings/ and seeds the extraction prompt over it", async () => {
    installWorkspace({
      detail: { docsFolderPath: "docs/base" },
    });
    const { starts } = installWorkbench();
    const created: Record<string, unknown>[] = [];
    server.use(
      http.post("/api/v1/workspaces/:id/docs", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        created.push(body);
        return HttpResponse.json(
          {
            data: createMockDocNode({
              id: "doc-transcript",
              name: String(body.name),
              docType: "design",
              filePath: String(body.filePath),
              workspaceId: RESOLVED_WORKSPACE_ID,
              repoId: "11111111-1111-4111-8111-111111111111",
            }),
          },
          { status: 201 },
        );
      }),
    );
    renderWorkspace();

    // The extraction template is a hidden action: the dialog starts it, the
    // ribbon never draws it.
    const review = await screen.findByTestId("ribbon-action-review-meeting");
    expect(
      screen.queryByTestId("ribbon-action-extract-decisions"),
    ).not.toBeInTheDocument();
    await waitFor(() => expect(review).toBeEnabled());
    await userEvent.click(review);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Review meeting");
    expect(
      within(dialog).getByTestId("transcript-source-existing"),
    ).toHaveTextContent("None stored under meetings/ yet.");

    await userEvent.upload(
      within(dialog).getByLabelText("Upload a transcript"),
      new File(
        ["Tim: the timeline is the landing page.\n"],
        "ui-discussion.txt",
        {
          type: "text/plain",
        },
      ),
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: "Upload and extract decisions",
      }),
    );

    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toMatchObject({
      filePath: "docs/base/meetings/ui-discussion.md",
    });
    // Upload plus extraction is one user action up to the composer: the BE
    // extraction prompt is seeded with the stored transcript as its current
    // document, and the person sends it (g5.workbench.chat-actions).
    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "extract-decisions")!,
      "be",
      { kind: "current_doc", docPath: "docs/base/meetings/ui-discussion.md" },
      "",
    );
    expect(expected).toContain(
      "Context: the current document is `docs/base/meetings/ui-discussion.md`.",
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue(expected));
    expect(starts).toHaveLength(0);
    const send = screen.getByTestId("chat-panel-no-session-send");
    await waitFor(() => expect(send).toBeEnabled());
    expect(send).toHaveAttribute("data-pulse", "true");
    await userEvent.click(send);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      persona: "be",
      phase: "be",
      content: expected,
      configRevision: WORKBENCH_REVISION,
    });
    expect(starts[0]).not.toHaveProperty("actionId");
    const pill = await screen.findByTestId("chat-phase-pill");
    await waitFor(() => expect(pill).toHaveAttribute("data-source", "persona"));
  });

  it("the chat pill shows the session persona and follow view forks under the current phase", async () => {
    installWorkspace({ phase: "se" });
    const { forks } = installWorkbench({ sessionPersona: "be" });
    renderWorkspace("/org-g5labs/test-workspace?session=chat-tgr-1");

    const pill = await screen.findByTestId("chat-phase-pill");
    await waitFor(() => expect(pill).toHaveAttribute("data-source", "persona"));
    expect(pill).toHaveTextContent("BE");
    const follow = screen.getByTestId("chat-follow-view");
    expect(follow).toHaveTextContent("SE");
    await userEvent.click(follow);

    await waitFor(() => expect(forks).toHaveLength(1));
    expect(forks[0]).toMatchObject({
      fromMessageIndex: 1,
      persona: "se",
      configRevision: WORKBENCH_REVISION,
    });
    expect(String(forks[0].requestKey)).toMatch(/^follow-view-/);
    // The fork opens and carries the new persona: nothing left to follow.
    await waitFor(() =>
      expect(screen.getByTestId("chat-phase-pill")).toHaveTextContent("SE"),
    );
    expect(screen.queryByTestId("chat-follow-view")).toBeNull();
  });

  it("shows the invalid-configuration banner with the built-ins, and the fallback with retry when the route fails", async () => {
    installWorkspace({});
    installWorkbench();
    server.use(
      http.get("/api/v1/workspaces/:id/workbench", () =>
        HttpResponse.json({
          data: {
            revision: "h.0",
            actions: WORKBENCH_ACTIONS,
            personas: {
              be: { source: "builtin", text: "be" },
              se: { source: "builtin", text: "se" },
              ie: { source: "builtin", text: "ie" },
            },
            config: {
              status: "invalid",
              error: ".g5/workbench/actions.yaml: duplicate action id `x`",
            },
          },
        }),
      ),
    );
    const first = renderWorkspace();
    expect(
      await screen.findByTestId("ribbon-config-invalid"),
    ).toHaveTextContent("duplicate action id `x`");
    expect(screen.getByTestId("ribbon-action-build-mock")).toBeEnabled();
    first.unmount();

    server.use(
      http.get("/api/v1/workspaces/:id/workbench", () =>
        HttpResponse.json(
          { error: { code: "INTERNAL_ERROR", message: "boom", details: {} } },
          { status: 500 },
        ),
      ),
    );
    renderWorkspace();
    expect(
      await screen.findByTestId("ribbon-config-fallback"),
    ).toHaveTextContent("Workbench configuration unavailable");
    expect(screen.getByTestId("ribbon-config-retry")).toBeInTheDocument();
    // Templated chats need a revision; `open` actions still work, and the
    // Build mock dialog greys its prompt with the ribbon's own reason.
    expect(screen.getByTestId("ribbon-action-new-doc")).toBeEnabled();
    const build = screen.getByTestId("ribbon-action-build-mock");
    expect(build).toBeEnabled();
    await userEvent.click(build);
    const submit = within(
      await screen.findByRole("dialog", { name: "Build mock" }),
    ).getByTestId("build-mock-submit");
    expect(submit).toBeDisabled();
    expect(submit).toHaveAttribute("title", RIBBON_FALLBACK_CHAT_TITLE);
  });

  it("a seed into the open BE chat shows the prompt there after a mock document was opened", async () => {
    installWorkspace({ detail: { docsFolderPath: "docs/base" } });
    const { starts } = installWorkbench({ sessionPersona: "be" });
    server.use(
      http.get("/api/v1/workspaces/:id/docs", () =>
        HttpResponse.json({
          data: [
            createMockDocNode({
              id: "mock-index",
              name: "Mock index",
              filePath: "docs/base/mocks/index.html",
              docType: "doc",
            }),
          ],
        }),
      ),
    );
    renderWorkspace("/org-g5labs/test-workspace?session=chat-tgr-1");
    const pill = await screen.findByTestId("chat-phase-pill");
    await waitFor(() => expect(pill).toHaveAttribute("data-source", "persona"));

    // Opening a mock puts the document in the center.
    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    await userEvent.click(build);
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Build mock" }),
      ).getByTestId("build-mock-open-mock-index"),
    );
    expect(
      await screen.findByRole("tab", { name: /Mock index/ }),
    ).toBeInTheDocument();

    // Build mock again: the seed lands in that BE chat, which is in view
    // with the prompt and a pulsing Send (review of PR #971).
    await userEvent.click(screen.getByTestId("ribbon-action-build-mock"));
    await userEvent.click(
      within(
        await screen.findByRole("dialog", { name: "Build mock" }),
      ).getByTestId("build-mock-submit"),
    );
    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "generate-mock")!,
      "be",
      { kind: "phase" },
      "",
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(expected),
    );
    expect(screen.getByTestId("chat-phase-pill")).toHaveTextContent("BE");
    expect(screen.getByTestId("chat-panel-send")).toHaveAttribute(
      "data-pulse",
      "true",
    );
    expect(starts).toHaveLength(0);
  });

  it("Build mock still seeds the built-in template when a committed phase lists the button without the hidden action", async () => {
    installWorkspace({});
    const { starts } = installWorkbench();
    // A committed file from before the dialog, or one naming only the drawn
    // actions: the BE Mock group carries `build-mock` and no `generate-mock`.
    const committed = {
      ...WORKBENCH_ACTIONS,
      be: WORKBENCH_ACTIONS.be.map((group) =>
        group.name === "Mock"
          ? {
              ...group,
              actions: group.actions.filter(
                (action) => action.id !== "generate-mock",
              ),
            }
          : group,
      ),
    };
    server.use(
      http.get("/api/v1/workspaces/:id/workbench", () =>
        HttpResponse.json({
          data: {
            revision: WORKBENCH_REVISION,
            actions: committed,
            personas: {
              be: { source: "builtin", text: "be" },
              se: { source: "builtin", text: "se" },
              ie: { source: "builtin", text: "ie" },
            },
            config: { status: "committed", error: null },
          },
        }),
      ),
    );
    renderWorkspace();

    const build = await screen.findByTestId("ribbon-action-build-mock");
    await waitFor(() => expect(build).toBeEnabled());
    await userEvent.click(build);
    const dialog = await screen.findByRole("dialog", { name: "Build mock" });
    const submit = within(dialog).getByTestId("build-mock-submit");
    expect(submit).toBeEnabled();
    await userEvent.click(submit);

    const expected = workbenchActionMessage(
      findWorkbenchAction(WORKBENCH_ACTIONS.be, "generate-mock")!,
      "be",
      { kind: "phase" },
      "",
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toHaveValue(expected));
    expect(starts).toHaveLength(0);
  });
});

describe("WorkspacePage — alignment deep links and persisted tabs", () => {
  it("opens a doc deep link beside the ribbon", async () => {
    installWorkspace({});
    installWorkbench();
    renderWorkspace("/org-g5labs/test-workspace/doc/doc-frontend-design");

    await screen.findByTestId("action-ribbon");
    await waitFor(() =>
      expect(
        within(screen.getByTestId("tab-bar")).getByText("Frontend Design"),
      ).toBeInTheDocument(),
    );
    const docTab = screen.getByRole("tab", { name: /Frontend Design/ });
    await waitFor(() =>
      expect(docTab).toHaveAttribute("aria-selected", "true"),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
  });

  it("shows a deep-linked doc that is already the persisted active tab", async () => {
    installWorkspace({});
    installWorkbench();
    // A reload of the doc's own URL: the tab comes back as the active one, so
    // nothing opens it, and the deep link still has to show it over Chat.
    localStorage.setItem(
      "g5:tabs:base",
      JSON.stringify({
        tabs: [
          {
            type: "doc",
            docId: "doc-frontend-design",
            name: "Frontend Design",
          },
        ],
        activeTabId: "doc:doc-frontend-design",
      }),
    );
    renderWorkspace("/org-g5labs/test-workspace/doc/doc-frontend-design");

    await screen.findByTestId("action-ribbon");
    const docTab = await screen.findByRole("tab", { name: /Frontend Design/ });
    await waitFor(() =>
      expect(docTab).toHaveAttribute("aria-selected", "true"),
    );
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toHaveAttribute("hidden"),
    );
  });

  it("reopens a durable doc deep link whose persisted live tab is no longer eligible", async () => {
    const workspaceId = "00000000-0000-4000-8000-000000000001";
    const repoId = "00000000-0000-4000-8000-000000000002";
    const filePath = `docs/${workspaceId}/restored-durable.md`;
    const doc = createMockDocNode({
      id: `overlay:${workspaceId}:${repoId}:${btoa(filePath).replace(/=/g, "")}`,
      workspaceId,
      repoId,
      filePath,
      name: "Restored durable doc",
      content: "# Restored durable document content",
      source: "committed",
    });
    installWorkspace({
      detail: {
        ...mockWorkspaceDetails[RESOLVED_WORKSPACE_ID],
        id: workspaceId,
        isBase: false,
        ownerUserId: "user-alex",
        readOnly: false,
        docsRepoId: repoId,
      },
    });
    installWorkbench();
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({ data: { orgId: "org-g5labs", workspaceId } }),
      ),
      http.get("/api/v1/workspaces/:id/docs", () =>
        HttpResponse.json({ data: [doc] }),
      ),
      http.get("/api/v1/workspaces/:id/files/*", ({ request }) => {
        const url = new URL(request.url);
        expect(decodeURIComponent(url.pathname)).toBe(
          `/api/v1/workspaces/${workspaceId}/files/${filePath}`,
        );
        expect(url.searchParams.get("repoId")).toBe(repoId);
        return HttpResponse.json({
          data: {
            path: filePath,
            content: doc.content,
            hash: doc.contentHash,
            size: new TextEncoder().encode(doc.content).length,
            encoding: "utf-8",
            source: "committed",
            repoOid: "durable-head",
            snapshotAt: null,
            vmId: null,
          },
        });
      }),
      http.get("/api/v1/workspaces/:id/vm", ({ params }) =>
        HttpResponse.json({
          data: createMockWorkspaceVm({
            workspaceId: String(params.id),
            ownerUserId: "user-alex",
            status: "stopped",
          }),
        }),
      ),
    );
    localStorage.setItem(
      `g5:tabs:${workspaceId}`,
      JSON.stringify({
        tabs: [
          {
            type: "doc",
            docId: doc.id,
            name: doc.name,
            liveAuthorityKey: "previous-running-attachment",
          },
        ],
        activeTabId: `doc:${doc.id}`,
      }),
    );
    renderWorkspace(
      `/org-g5labs/test-workspace/doc/${encodeURIComponent(doc.id)}`,
    );
    expect(
      await screen.findByRole("heading", {
        name: "Restored durable document content",
      }),
    ).toBeVisible();
    expect(screen.queryByTestId("empty-workspace-getting-started")).toBeNull();
  });

  it("opens a file deep link beside the ribbon", async () => {
    installWorkspace({});
    installWorkbench();
    renderWorkspace("/org-g5labs/test-workspace/file/src/App.tsx");

    await screen.findByTestId("action-ribbon");
    const fileTab = await screen.findByRole("tab", { name: /App\.tsx/ });
    await waitFor(() =>
      expect(fileTab).toHaveAttribute("aria-selected", "true"),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
  });

  it("restores persisted tabs beside the pinned timeline and the ribbon", async () => {
    installWorkspace({});
    installWorkbench();
    localStorage.setItem(
      "g5:tabs:base",
      JSON.stringify({
        tabs: [{ type: "workspace_details", name: "Workspace details" }],
        activeTabId: "workspace-details",
      }),
    );
    renderWorkspace();

    await screen.findByTestId("action-ribbon");
    const tabBar = await screen.findByTestId("tab-bar");
    expect(
      await within(tabBar).findByText("Workspace details"),
    ).toBeInTheDocument();
    expect(screen.getAllByTestId("center-content-tab")[0]).toHaveTextContent(
      "Workspace",
    );
  });
});

/** Shows the current route so a test can assert what the page wrote. */
const RouteProbe = () => {
  const { pathname, search } = useLocation();
  return <div data-testid="route-probe">{`${pathname}${search}`}</div>;
};
/** Stands in for an in-page link to a doc (a reference, a feed row). */
const DocLink = () => {
  const navigate = useNavigate();
  return (
    <button
      type="button"
      data-testid="doc-link"
      onClick={() =>
        navigate("/org-g5labs/test-workspace/doc/doc-frontend-design")
      }
    >
      doc
    </button>
  );
};
/** A tab of the center tab bar by its accessible name. */
const centerTab = (name: RegExp) =>
  within(screen.getByTestId("tab-bar")).getByRole("tab", { name });

describe("WorkspacePage — open workspaces are tabs (g5.workbench.tabs)", () => {
  /** Stands in for a Home row or notification linking into the workspace. */
  const HomeDetailsLink = () => {
    const navigate = useNavigate();
    return (
      <button
        type="button"
        data-testid="home-details-link"
        onClick={() => navigate("/org-g5labs/test-workspace?view=details")}
      >
        details
      </button>
    );
  };
  function renderWithTabs(route = "/org-g5labs/test-workspace") {
    return renderApp(
      <WorkspaceTabsProvider>
        <RouteProbe />
        <DocLink />
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
          <Route
            path="/:org/home"
            element={
              <AppShell>
                <div data-testid="org-home" />
                <HomeDetailsLink />
              </AppShell>
            }
          />
        </Routes>
      </WorkspaceTabsProvider>,
      { route, user: createMockUser() },
    );
  }

  it("shows the strip with the workspace's tab and drops the top-bar Home button", async () => {
    installWorkspace({
      features: [],
    });
    renderWithTabs();
    const strip = await screen.findByTestId("workspace-tab-strip");
    expect(
      await within(strip).findByRole("tab", { name: "test-workspace" }),
    ).toHaveAttribute("aria-selected", "true");
    await screen.findByTestId("phase-chevrons");
    expect(screen.queryByTestId("top-bar-home")).toBeNull();
  });

  it("restores each workspace's own center tabs when switching tabs and back", async () => {
    installWorkspace({});
    installWorkbench();
    // Only Task Graph Rework has a persisted center tab.
    localStorage.setItem(
      "g5:tabs:task-graph-rework",
      JSON.stringify({
        tabs: [{ type: "workspace_details", name: "Workspace details" }],
        activeTabId: "workspace-details",
      }),
    );
    localStorage.setItem(
      workspaceTabsKey("user-alex"),
      JSON.stringify([
        { orgSlug: "org-g5labs", name: "test-workspace" },
        { orgSlug: "org-g5labs", name: "task-graph-rework" },
      ]),
    );
    const user = userEvent.setup();
    renderWithTabs();
    // The page remounts its shell per workspace, so the strip is re-queried
    // after every switch.
    const stripTab = (name: string) =>
      within(screen.getByTestId("workspace-tab-strip")).getByRole("tab", {
        name,
      });
    const detailsTab = () =>
      within(screen.getByTestId("tab-bar")).queryByText("Workspace details");
    await screen.findByTestId("workspace-tab-strip");
    await screen.findByTestId("action-ribbon");
    await screen.findByTestId("tab-bar");
    expect(detailsTab()).toBeNull();

    await user.click(stripTab("task-graph-rework"));
    await waitFor(() => expect(detailsTab()).toBeInTheDocument());

    await user.click(stripTab("test-workspace"));
    await waitFor(() =>
      expect(stripTab("test-workspace")).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await waitFor(() => expect(detailsTab()).toBeNull());

    await user.click(stripTab("task-graph-rework"));
    await waitFor(() => expect(detailsTab()).toBeInTheDocument());
  });

  it("returns to the center tab a workspace was on, not the timeline; a fresh open lands again", async () => {
    installWorkspace({});
    installWorkbench();
    // test-workspace (id `base`) has a route-less Pull Requests tab persisted
    // as the selected one; the landing still shows the timeline on a fresh
    // open.
    localStorage.setItem(
      "g5:tabs:base",
      JSON.stringify({
        tabs: [{ type: "pull_requests", name: "Pull Requests" }],
        activeTabId: "pull-requests",
      }),
    );
    localStorage.setItem(
      workspaceTabsKey("user-alex"),
      JSON.stringify([
        { orgSlug: "org-g5labs", name: "test-workspace" },
        { orgSlug: "org-g5labs", name: "task-graph-rework" },
      ]),
    );
    const user = userEvent.setup();
    const rendered = renderWithTabs();
    const stripTab = (name: string) =>
      within(screen.getByTestId("workspace-tab-strip")).getByRole("tab", {
        name,
      });
    await screen.findByTestId("workspace-tab-strip");
    await screen.findByTestId("workspace-landing");
    expect(centerTab(/^Workspace$/)).toHaveAttribute("aria-selected", "true");
    // The landing's `?view=activity` mirror lands a render later.
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "/org-g5labs/test-workspace?view=activity",
      ),
    );

    // Pick Pull Requests: the URL names it at the workspace root
    // (`?view=pull-requests`, issue #561) and the selection is persisted.
    await user.click(centerTab(/Pull Requests/));
    await waitFor(() =>
      expect(centerTab(/Pull Requests/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=pull-requests$/,
      ),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();

    // Away and back: Pull Requests again, not the timeline — even though
    // another browser window wrote Activity as the selection meanwhile (the
    // strip's session snapshot, not the shared record, is what returns).
    await user.click(stripTab("task-graph-rework"));
    await screen.findByTestId("workspace-landing");
    localStorage.setItem(
      "g5:tabs:base",
      JSON.stringify({
        tabs: [
          { type: "workspace_activity", name: "Activity" },
          { type: "pull_requests", name: "Pull Requests" },
        ],
        activeTabId: "workspace-activity",
      }),
    );
    await user.click(stripTab("test-workspace"));
    await waitFor(() =>
      expect(stripTab("test-workspace")).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await waitFor(() =>
      expect(centerTab(/Pull Requests/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();

    // Through Home as well: the page unmounts and mounts again.
    await user.click(
      within(screen.getByTestId("workspace-tab-strip")).getByTestId(
        "workspace-tab-home",
      ),
    );
    await screen.findByTestId("org-home");
    await user.click(stripTab("test-workspace"));
    await waitFor(() =>
      expect(centerTab(/Pull Requests/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();

    // A link into the workspace is an opening: what it names wins over the
    // remembered selection.
    await user.click(
      within(screen.getByTestId("workspace-tab-strip")).getByTestId(
        "workspace-tab-home",
      ),
    );
    await screen.findByTestId("org-home");
    await user.click(screen.getByTestId("home-details-link"));
    const details = await screen.findByRole("tab", {
      name: /Workspace Details/,
    });
    await waitFor(() =>
      expect(details).toHaveAttribute("aria-selected", "true"),
    );
    await waitFor(() =>
      expect(centerTab(/Pull Requests/)).toHaveAttribute(
        "aria-selected",
        "false",
      ),
    );

    await user.click(centerTab(/^Workspace$/));
    await screen.findByTestId("workspace-landing");
    expect(
      await screen.findByRole("tab", { name: /Pull Requests/ }),
    ).toHaveAttribute("aria-selected", "false");

    // A fresh page (a reload): memory is gone, the timeline lands again.
    rendered.unmount();
    renderWithTabs();
    await screen.findByTestId("workspace-tab-strip");
    await screen.findByTestId("workspace-landing");
    expect(
      within(screen.getByTestId("tab-bar")).getByRole("tab", {
        name: /^Workspace$/,
      }),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("returns to Chat in the center when it was the center", async () => {
    installWorkspace({});
    installWorkbench();
    localStorage.setItem(
      workspaceTabsKey("user-alex"),
      JSON.stringify([
        { orgSlug: "org-g5labs", name: "test-workspace" },
        { orgSlug: "org-g5labs", name: "task-graph-rework" },
      ]),
    );
    const user = userEvent.setup();
    renderWithTabs("/org-g5labs/test-workspace?session=chat-tgr-1");
    const stripTab = (name: string) =>
      within(screen.getByTestId("workspace-tab-strip")).getByRole("tab", {
        name,
      });
    await screen.findByTestId("chat-phase-pill");
    expect(screen.getByTestId("right-panel")).toHaveAttribute(
      "data-chat-placement",
      "center",
    );

    await user.click(stripTab("task-graph-rework"));
    await screen.findByTestId("workspace-landing");
    await user.click(stripTab("test-workspace"));
    await waitFor(() =>
      expect(stripTab("test-workspace")).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("right-panel")).toHaveAttribute(
        "data-chat-placement",
        "center",
      ),
    );
    // The pinned Activity tab stays open behind Chat, not shown.
    const header = screen.queryByTestId("workspace-landing");
    if (header) expect(header).not.toBeVisible();
    expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
      "hidden",
    );
  });

  it("returns to Chat in the center over a doc route when it was the center", async () => {
    installWorkspace({});
    installWorkbench();
    localStorage.setItem(
      workspaceTabsKey("user-alex"),
      JSON.stringify([
        { orgSlug: "org-g5labs", name: "test-workspace" },
        { orgSlug: "org-g5labs", name: "task-graph-rework" },
      ]),
    );
    const user = userEvent.setup();
    renderWithTabs("/org-g5labs/test-workspace/doc/doc-frontend-design");
    const stripTab = (name: string) =>
      within(screen.getByTestId("workspace-tab-strip")).getByRole("tab", {
        name,
      });
    await screen.findByTestId("workspace-tab-strip");
    const docTab = await screen.findByRole("tab", { name: /Frontend Design/ });
    await waitFor(() =>
      expect(docTab).toHaveAttribute("aria-selected", "true"),
    );
    // Chat over the doc, no session hint in the URL.
    await user.click(
      within(screen.getByTestId("tab-bar")).getByTestId("center-chat-tab"),
    );
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
        "hidden",
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/doc\/doc-frontend-design$/,
      ),
    );

    await user.click(stripTab("task-graph-rework"));
    await screen.findByTestId("workspace-landing");
    await user.click(stripTab("test-workspace"));
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "/org-g5labs/test-workspace/doc/doc-frontend-design",
      ),
    );
    // The doc route hydrates behind Chat, which stays in the center.
    expect(
      await screen.findByRole("tab", { name: /Frontend Design/ }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
        "hidden",
      ),
    );
    await act(() => new Promise((resolve) => setTimeout(resolve, 200)));
    expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
      "hidden",
    );

    // Later in the visit: Activity, then a link back to the same doc shows
    // the doc — the return guard has expired.
    await user.click(
      within(screen.getByTestId("tab-bar")).getByRole("tab", {
        name: /^Workspace$/,
      }),
    );
    await screen.findByTestId("workspace-landing");
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    await user.click(screen.getByTestId("doc-link"));
    await waitFor(() =>
      expect(
        screen.getByRole("tab", { name: /Frontend Design/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
  });
});

describe("WorkspacePage — center surface URLs (g5.workbench.tabs)", () => {
  const workspaceRoutes = (element: ReactNode) => (
    <>
      <Route path="/:org/:workspace/task/:taskId" element={element} />
      <Route path="/:org/:workspace/*" element={element} />
    </>
  );
  function renderProbed(route = "/org-g5labs/test-workspace") {
    return renderApp(
      <>
        <RouteProbe />
        <Routes>{workspaceRoutes(<WorkspacePage />)}</Routes>
      </>,
      { route, user: createMockUser() },
    );
  }
  const probe = () => screen.getByTestId("route-probe");
  /** `test-workspace` persisted with these tabs, Activity landing first. */
  function persistTabs(tabs: unknown[], activeTabId: string | null = null) {
    localStorage.setItem("g5:tabs:base", JSON.stringify({ tabs, activeTabId }));
  }
  // The missing commit diff has a visible error; this suite tests its URL.
  function serveMissingCommitDiff() {
    server.use(
      http.get("/api/v1/workspaces/:id/repos/:repoId/commits/:sha/diff", () =>
        HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "no diff", details: {} } },
          { status: 404 },
        ),
      ),
    );
  }

  it("opens Pull Requests from ?view=pull-requests over the landing and mirrors the selection", async () => {
    installWorkspace({});
    installWorkbench();
    renderProbed("/org-g5labs/test-workspace?view=pull-requests");

    await screen.findByTestId("phase-chevrons");
    const pulls = await screen.findByRole("tab", { name: /Pull Requests/ });
    await waitFor(() => expect(pulls).toHaveAttribute("aria-selected", "true"));
    expect(screen.queryByTestId("workspace-landing")).toBeNull();

    // Another tab clears the hint; picking Pull Requests again writes it.
    await userEvent.click(centerTab(/^Workspace$/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    await userEvent.click(centerTab(/Pull Requests/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=pull-requests$/,
      ),
    );
  });

  it.each(["progress", "unavailable-view"])(
    "recovers an unavailable %s deep link through the workspace fallback",
    async (view) => {
      installWorkspace({});
      installWorkbench();
      renderProbed(`/org-g5labs/test-workspace?view=${view}`);
      expect(await screen.findByTestId("workspace-landing")).toBeVisible();
      expect(screen.queryByRole("tab", { name: /Progress/ })).toBeNull();
      await waitFor(() => expect(probe()).toHaveTextContent("view=activity"));
    },
  );

  it("keeps an older task deep link canonical while opening its default changes tab", async () => {
    installWorkspace({});
    installWorkbench();
    const task = createMockTask({
      id: "older-task",
      workspaceId: "base",
      status: "succeeded",
    });
    server.use(
      http.get("/api/v1/workspaces/:id/tasks", () =>
        HttpResponse.json({
          data: { tasks: [], cursor: null, hasMore: false },
        }),
      ),
      http.get("/api/v1/tasks/older-task", () =>
        HttpResponse.json({ data: task }),
      ),
    );
    renderProbed("/org-g5labs/test-workspace/task/older-task");
    await screen.findByTestId("phase-chevrons");
    await waitFor(() =>
      expect(centerTab(/^Workspace changes$/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/task\/older-task$/,
      ),
    );
  });

  it("reopens Current changes from its URL and mirrors selection from a content path", async () => {
    installWorkspace({});
    installWorkbench();
    const rendered = renderProbed("/org-g5labs/test-workspace?view=changes");
    await screen.findByTestId("phase-chevrons");
    await waitFor(() =>
      expect(centerTab(/^Current changes$/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await userEvent.click(centerTab(/^Workspace$/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    await userEvent.click(centerTab(/^Current changes$/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=changes$/,
      ),
    );
    rendered.unmount();

    renderProbed("/org-g5labs/test-workspace/guide/getting-started");
    await screen.findByTestId("phase-chevrons");
    await waitFor(() =>
      expect(centerTab(/Getting Started/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await userEvent.click(centerTab(/^Current changes$/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=changes$/,
      ),
    );
  });

  it("opens Source Control from ?view=source-control and the Diff Graph from ?view=diff-graph", async () => {
    installWorkspace({});
    installWorkbench();
    const rendered = renderProbed(
      "/org-g5labs/test-workspace?view=source-control",
    );
    await screen.findByTestId("phase-chevrons");
    await waitFor(() =>
      expect(centerTab(/Source Control/)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    rendered.unmount();
    localStorage.removeItem("g5:tabs:base");

    renderProbed("/org-g5labs/test-workspace?view=diff-graph");
    await screen.findByTestId("phase-chevrons");
    await waitFor(() =>
      expect(centerTab(/Diff Graph/)).toHaveAttribute("aria-selected", "true"),
    );
    expect(probe()).toHaveTextContent(
      /^\/org-g5labs\/test-workspace\?view=diff-graph$/,
    );
  });

  it("opens nothing for ?view=diff-graph on Base and lands on the timeline, over a persisted selection", async () => {
    installWorkspace({ isBase: true });
    installWorkbench();
    persistTabs(
      [
        {
          type: "guide_doc",
          guideId: "getting-started",
          name: "Getting Started",
        },
      ],
      "guide:getting-started",
    );
    renderProbed("/org-g5labs/test-workspace?view=diff-graph");
    await screen.findByTestId("workspace-landing");
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    expect(
      screen
        .getByTestId("tab-bar")
        .querySelector("#center-content-tab-workspace-activity"),
    ).toHaveAttribute("aria-selected", "true");
    expect(
      within(screen.getByTestId("tab-bar")).queryByRole("tab", {
        name: /Diff Graph/,
      }),
    ).toBeNull();
    expect(screen.queryByTestId("guide-doc-view")).toBeNull();
  });

  it("opens a guide from /guide/:guideId and writes the path on selection; an unknown guide falls back to the root", async () => {
    installWorkspace({});
    installWorkbench();
    const rendered = renderProbed(
      "/org-g5labs/test-workspace/guide/getting-started",
    );
    await screen.findByTestId("phase-chevrons");
    await screen.findByTestId("guide-doc-view");
    const guide = await screen.findByRole("tab", { name: /Getting Started/i });
    await waitFor(() => expect(guide).toHaveAttribute("aria-selected", "true"));
    expect(screen.queryByTestId("workspace-landing")).toBeNull();

    await userEvent.click(centerTab(/^Workspace$/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    await userEvent.click(centerTab(/Getting Started/i));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/guide\/getting-started$/,
      ),
    );
    rendered.unmount();
    localStorage.removeItem("g5:tabs:base");

    // A stale link lands like a fresh open: the timeline, even over a
    // persisted guide selection, which must not stand in for the link.
    persistTabs(
      [
        {
          type: "guide_doc",
          guideId: "getting-started",
          name: "Getting Started",
        },
      ],
      "guide:getting-started",
    );
    renderProbed("/org-g5labs/test-workspace/guide/no-such-guide");
    await screen.findByTestId("workspace-landing");
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    expect(centerTab(/^Workspace$/)).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByTestId("guide-doc-view")).toBeNull();
  });

  it("opens an external issue from /issue/:repositoryId/:number, labelled by the workspace's link", async () => {
    installWorkspace({
      features: ["external_issues"],
    });
    installWorkbench();
    serveOrgExternalIssues();
    const issue = makeIssueDetail();
    const link = makeIssueLink({ workspaceId: RESOLVED_WORKSPACE_ID, issue });
    server.use(
      http.get("/api/v1/workspaces/:id/issue-links", () =>
        HttpResponse.json({ data: { links: [link] } }),
      ),
      http.get("/api/v1/orgs/:orgId/issues/github/11/:number", () =>
        HttpResponse.json({ data: issue }),
      ),
    );
    // The issue read is org-scoped: `g5labs` is the org the mocks serve.
    const rendered = renderProbed("/g5labs/test-workspace/issue/11/12");
    await screen.findByTestId("phase-chevrons");
    await screen.findByTestId("external-issue-detail");
    const issueTab = await screen.findByRole("tab", { name: /acme\/api#12/ });
    await waitFor(() =>
      expect(issueTab).toHaveAttribute("aria-selected", "true"),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
    rendered.unmount();
    localStorage.removeItem("g5:tabs:base");

    // An issue the workspace does not link is labelled by its number.
    renderProbed("/g5labs/test-workspace/issue/11/13");
    await screen.findByTestId("phase-chevrons");
    const unlinked = await screen.findByRole("tab", { name: /#13/ });
    await waitFor(() =>
      expect(unlinked).toHaveAttribute("aria-selected", "true"),
    );
  });

  it("falls back to the root for an issue link when the organization offers no external issues", async () => {
    installWorkspace({
      features: ["external_issues"],
    });
    installWorkbench();
    serveOrgExternalIssues(false);
    renderProbed("/g5labs/test-workspace/issue/11/12");
    await screen.findByTestId("workspace-landing");
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/g5labs\/test-workspace\?view=activity$/,
      ),
    );
    expect(
      within(screen.getByTestId("tab-bar")).queryByRole("tab", {
        name: /#12/,
      }),
    ).toBeNull();
  });

  it("keeps the workspace tab strip on the new routes and remembers them for the return", async () => {
    installWorkspace({});
    installWorkbench();
    localStorage.setItem(
      workspaceTabsKey("user-alex"),
      JSON.stringify([{ orgSlug: "org-g5labs", name: "test-workspace" }]),
    );
    renderApp(
      <WorkspaceTabsProvider>
        <RouteProbe />
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>
      </WorkspaceTabsProvider>,
      {
        route: "/org-g5labs/test-workspace/guide/getting-started",
        user: createMockUser(),
      },
    );
    const strip = await screen.findByTestId("workspace-tab-strip");
    await screen.findByTestId("guide-doc-view");
    expect(
      within(strip).getByRole("tab", { name: "test-workspace" }),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("opens a commit diff from /commit/:repoId/:sha, replacing a persisted commit, and falls back to the root for an unknown repository", async () => {
    installWorkspace({});
    installWorkbench();
    serveMissingCommitDiff();
    const sha = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0";
    // The commit tab is a singleton: a persisted, selected commit A must
    // give way to the linked commit B, not keep A's identity under B's URL.
    persistTabs(
      [
        {
          type: "source_control_commit_diff",
          name: "9999999",
          repoId: "repo-g5-client",
          repoName: "g5-client",
          branchId: "repo-g5-client",
          sha: "9".repeat(40),
        },
      ],
      "source-control-commit-diff",
    );
    const rendered = renderProbed(
      `/org-g5labs/test-workspace/commit/repo-g5-client/${sha}`,
    );
    await screen.findByTestId("phase-chevrons");
    const commit = await screen.findByRole("tab", { name: /a1b2c3d/ });
    await waitFor(() =>
      expect(commit).toHaveAttribute("aria-selected", "true"),
    );
    expect(
      within(screen.getByTestId("tab-bar")).queryByRole("tab", {
        name: /9999999/,
      }),
    ).toBeNull();
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
    rendered.unmount();
    localStorage.removeItem("g5:tabs:base");

    renderProbed(`/org-g5labs/test-workspace/commit/repo-unknown/${sha}`);
    await screen.findByTestId("workspace-landing");
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
  });

  it("hydrates a commit link over a persisted commit under ?view=activity and still follows a later link", async () => {
    installWorkspace({});
    installWorkbench();
    serveMissingCommitDiff();
    const sha = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0";
    persistTabs(
      [
        {
          type: "source_control_commit_diff",
          name: "9999999",
          repoId: "repo-g5-client",
          repoName: "g5-client",
          branchId: "repo-g5-client",
          sha: "9".repeat(40),
        },
      ],
      "source-control-commit-diff",
    );
    renderApp(
      <>
        <RouteProbe />
        <DocLink />
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>
      </>,
      {
        route: `/org-g5labs/test-workspace/commit/repo-g5-client/${sha}?view=activity`,
        user: createMockUser(),
      },
    );
    await screen.findByTestId("phase-chevrons");
    const commit = await screen.findByRole("tab", { name: /a1b2c3d/ });
    await waitFor(() =>
      expect(commit).toHaveAttribute("aria-selected", "true"),
    );
    // The stale hint is gone and the awaited URL is the one that landed.
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        new RegExp(`^/org-g5labs/test-workspace/commit/repo-g5-client/${sha}$`),
      ),
    );
    await userEvent.click(screen.getByTestId("doc-link"));
    const doc = await screen.findByRole("tab", { name: /Frontend Design/ });
    await waitFor(() => expect(doc).toHaveAttribute("aria-selected", "true"));
  });

  it("drops a stale center-view hint beside a guide path and still follows a later link", async () => {
    installWorkspace({});
    installWorkbench();
    renderApp(
      <>
        <RouteProbe />
        <DocLink />
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>
      </>,
      {
        route: "/org-g5labs/test-workspace/guide/getting-started?view=activity",
        user: createMockUser(),
      },
    );
    await screen.findByTestId("phase-chevrons");
    await screen.findByTestId("guide-doc-view");
    await waitFor(() =>
      expect(centerTab(/Getting Started/i)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/guide\/getting-started$/,
      ),
    );
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
    await userEvent.click(screen.getByTestId("doc-link"));
    const doc = await screen.findByRole("tab", { name: /Frontend Design/ });
    await waitFor(() => expect(doc).toHaveAttribute("aria-selected", "true"));
  });

  it("shows a persisted, selected Source Control tab on a ?view=source-control reload with its file and Chat out of the center", async () => {
    installWorkspace({});
    installWorkbench();
    const selectedFileKey = "repo-g5-client\u0000src/a.ts";
    persistTabs(
      [
        {
          type: "source_control_diff",
          name: "Changes",
          selectedFileKey,
        },
      ],
      "source-control-diff",
    );
    renderProbed("/org-g5labs/test-workspace?view=source-control");
    await screen.findByTestId("phase-chevrons");
    await waitFor(() =>
      expect(centerTab(/Changes/)).toHaveAttribute("aria-selected", "true"),
    );
    expect(
      screen.queryByRole("tab", { name: /Chat/, selected: true }),
    ).toBeNull();
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
    // Wait for observable saved selections, so the fixture's original record
    // cannot satisfy the assertion before the debounced writer runs.
    await userEvent.click(centerTab(/^(Workspace|Activity)$/));
    await waitFor(() =>
      expect(
        JSON.parse(localStorage.getItem("g5:tabs:base") ?? "{}").activeTabId,
      ).toBe("workspace-activity"),
    );
    await userEvent.click(centerTab(/Changes/));
    await waitFor(() => {
      const record = JSON.parse(localStorage.getItem("g5:tabs:base") ?? "{}");
      expect(record.activeTabId).toBe("source-control-diff");
      expect(
        record.tabs.find(
          (tab: { type: string }) => tab.type === "source_control_diff",
        ).selectedFileKey,
      ).toBe(selectedFileKey);
    });
  });

  it("follows a doc link carrying branch and view hints after a guide, and a later link again", async () => {
    installWorkspace({});
    installWorkbench();
    const HintedDocLink = () => {
      const navigate = useNavigate();
      return (
        <button
          type="button"
          data-testid="hinted-doc-link"
          onClick={() =>
            navigate(
              "/org-g5labs/test-workspace/doc/doc-frontend-design?branch=repo-g5-client&view=activity",
            )
          }
        >
          hinted doc
        </button>
      );
    };
    const FileLink = () => {
      const navigate = useNavigate();
      return (
        <button
          type="button"
          data-testid="file-link"
          onClick={() =>
            navigate("/org-g5labs/test-workspace/file/src/App.tsx")
          }
        >
          file
        </button>
      );
    };
    renderApp(
      <>
        <RouteProbe />
        <HintedDocLink />
        <FileLink />
        <Routes>{workspaceRoutes(<WorkspacePage />)}</Routes>
      </>,
      {
        route: "/org-g5labs/test-workspace/guide/getting-started",
        user: createMockUser(),
      },
    );
    await screen.findByTestId("guide-doc-view");
    await userEvent.click(screen.getByTestId("hinted-doc-link"));
    const doc = await screen.findByRole("tab", { name: /Frontend Design/ });
    await waitFor(() => expect(doc).toHaveAttribute("aria-selected", "true"));
    // The doc's own write (no branch on a doc route, hint dropped) is the
    // URL that stands; nothing rewrites it from the link's query.
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/doc\/doc-frontend-design$/,
      ),
    );
    await userEvent.click(screen.getByTestId("file-link"));
    const file = await screen.findByRole("tab", { name: /App\.tsx/ });
    await waitFor(() => expect(file).toHaveAttribute("aria-selected", "true"));
  });

  it("keeps a Source Control tab's selected file when its ?view= hint is mirrored", async () => {
    installWorkspace({});
    installWorkbench();
    const selectedFileKey = "repo-g5-client\u0000src/a.ts";
    persistTabs([
      { type: "source_control_diff", name: "Changes", selectedFileKey },
    ]);
    renderProbed();
    await screen.findByTestId("workspace-landing");
    await userEvent.click(centerTab(/Changes/));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=source-control$/,
      ),
    );
    await waitFor(() => {
      const record = JSON.parse(localStorage.getItem("g5:tabs:base") ?? "{}");
      expect(record.activeTabId).toBe("source-control-diff");
      expect(
        record.tabs.find(
          (tab: { type: string }) => tab.type === "source_control_diff",
        ).selectedFileKey,
      ).toBe(selectedFileKey);
    });
  });

  it("keeps the opener's commit title when the route names the commit already shown", async () => {
    installWorkspace({});
    installWorkbench();
    serveMissingCommitDiff();
    const sha = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0";
    persistTabs(
      [
        {
          type: "source_control_commit_diff",
          name: "Fix the thing",
          repoId: "repo-g5-client",
          repoName: "g5-client",
          branchId: "repo-g5-client",
          sha,
        },
      ],
      "source-control-commit-diff",
    );
    renderProbed(`/org-g5labs/test-workspace/commit/repo-g5-client/${sha}`);
    await screen.findByTestId("phase-chevrons");
    const commit = await screen.findByRole("tab", { name: /Fix the thing/ });
    await waitFor(() =>
      expect(commit).toHaveAttribute("aria-selected", "true"),
    );
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(
      within(screen.getByTestId("tab-bar")).queryByRole("tab", {
        name: /a1b2c3d/,
      }),
    ).toBeNull();
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
  });

  it("opens the guide in another workspace linked to the same guide this one shows", async () => {
    installWorkspace({});
    installWorkbench();
    persistTabs(
      [
        {
          type: "guide_doc",
          guideId: "getting-started",
          name: "Getting Started",
        },
      ],
      "guide:getting-started",
    );
    const OtherGuideLink = () => {
      const navigate = useNavigate();
      return (
        <button
          type="button"
          data-testid="other-guide-link"
          onClick={() =>
            navigate("/org-g5labs/task-graph-rework/guide/getting-started")
          }
        >
          other
        </button>
      );
    };
    renderApp(
      <>
        <RouteProbe />
        <OtherGuideLink />
        <Routes>{workspaceRoutes(<WorkspacePage />)}</Routes>
      </>,
      {
        route: "/org-g5labs/test-workspace/guide/getting-started",
        user: createMockUser(),
      },
    );
    await screen.findByTestId("guide-doc-view");
    await waitFor(() =>
      expect(centerTab(/Getting Started/i)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    await userEvent.click(screen.getByTestId("other-guide-link"));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/task-graph-rework\/guide\/getting-started$/,
      ),
    );
    // The other workspace restores no tabs of its own; the link still shows
    // its guide once that restoration has committed.
    await waitFor(() =>
      expect(centerTab(/Getting Started/i)).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    expect(screen.getByTestId("guide-doc-view")).toBeInTheDocument();
  });

  it("falls back to the root for an unknown guide even beside a view hint", async () => {
    installWorkspace({});
    installWorkbench();
    renderProbed(
      "/org-g5labs/test-workspace/guide/no-such-guide?view=pull-requests",
    );
    await screen.findByTestId("workspace-landing");
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=activity$/,
      ),
    );
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(probe()).toHaveTextContent(
      /^\/org-g5labs\/test-workspace\?view=activity$/,
    );
    expect(
      within(screen.getByTestId("tab-bar")).queryByRole("tab", {
        name: /Pull Requests/,
      }),
    ).toBeNull();
    expect(screen.queryByTestId("guide-doc-view")).toBeNull();
  });

  it("mirrors a review copy's Review landing over a guide link, superseding the guide's URL write", async () => {
    installWorkspace({});
    installWorkbench();
    server.use(
      http.get("/api/v1/workspaces/:id/review", () =>
        HttpResponse.json({
          data: {
            asSource: null,
            asReview: {
              forkId: "fork-1",
              orgId: "org-g5labs",
              sourceWorkspaceId: "ws-source-1",
              sourceWorkspaceName: "kaihui-test3",
              reviewWorkspaceId: "base",
              reviewWorkspaceName: "kaihui-test3-review-alex-chen",
              reviewerUserId: "user-alex",
              reviewerDisplayName: "Alex Chen",
              vmStatus: "stopped",
              vmStartedAt: null,
              vmLastActivityAt: null,
              lastFlushedAt: null,
              createdAt: "2026-09-22T16:10:00.000Z",
            },
            openSuggestions: 0,
          },
        }),
      ),
    );
    renderProbed(
      "/org-g5labs/test-workspace/guide/getting-started?view=activity",
    );
    await screen.findByTestId("phase-chevrons");
    // The review copy lands on its Review tab once per load (its rule);
    // the guide's own URL write is superseded, not waited for: the URL
    // mirrors Review.
    const review = await screen.findByRole("tab", {
      name: /Review of kaihui-test3/,
    });
    await waitFor(() =>
      expect(review).toHaveAttribute("aria-selected", "true"),
    );
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\?view=review$/,
      ),
    );
  });

  it("hands the center to the guide when a same-path link drops the conversation hint", async () => {
    installWorkspace({});
    installWorkbench();
    const BareGuideLink = () => {
      const navigate = useNavigate();
      return (
        <button
          type="button"
          data-testid="bare-guide-link"
          onClick={() =>
            navigate("/org-g5labs/test-workspace/guide/getting-started")
          }
        >
          guide
        </button>
      );
    };
    renderApp(
      <>
        <RouteProbe />
        <BareGuideLink />
        <Routes>{workspaceRoutes(<WorkspacePage />)}</Routes>
      </>,
      {
        route:
          "/org-g5labs/test-workspace/guide/getting-started?session=chat-tgr-1",
        user: createMockUser(),
      },
    );
    // The conversation hint keeps Chat in the center over the guide.
    await screen.findByTestId("chat-phase-pill");
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
        "hidden",
      ),
    );
    await userEvent.click(screen.getByTestId("bare-guide-link"));
    await waitFor(() =>
      expect(probe()).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/guide\/getting-started$/,
      ),
    );
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toHaveAttribute("hidden"),
    );
    expect(screen.getByTestId("guide-doc-view")).toBeInTheDocument();
    expect(centerTab(/Getting Started/i)).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});

describe("WorkspacePage — Phase 8 rail and linked issue actions", () => {
  function installIssues(
    options: Parameters<typeof installWorkspace>[0] = {
      phase: "se",
    },
  ) {
    installWorkspace({
      ...options,
      features: ["external_issues"],
    });
    serveOrgExternalIssues();
    const issue = makeIssueDetail();
    const link = makeIssueLink({ workspaceId: RESOLVED_WORKSPACE_ID, issue });
    let links = [link];
    const unlinked: string[] = [];
    server.use(
      http.get("/api/v1/workspaces/:id/issue-links", () =>
        HttpResponse.json({ data: { links } }),
      ),
      http.get("/api/v1/orgs/:orgId/issues/github/11/12", () =>
        HttpResponse.json({ data: issue }),
      ),
      http.post(
        "/api/v1/workspaces/:id/issue-links/:linkId/unlink",
        ({ params }) => {
          unlinked.push(String(params.linkId));
          links = [];
          return HttpResponse.json({ data: { unlinked: true } });
        },
      ),
    );
    return { issue, link, unlinked };
  }
  async function openIssue() {
    await userEvent.click(
      await screen.findByRole("button", { name: "Issues & tasks" }),
    );
    await userEvent.click(
      await screen.findByTestId("workspace-issues-row-link-1"),
    );
    await screen.findByTestId("external-issue-detail");
  }
  it("the ribbon's Overview control leads the row before New doc and opens the workspace page; the rail has no Activity section; the Issues section only in alignment (g5.workbench.landing)", async () => {
    installIssues();
    renderWorkspace("/g5labs/test-workspace?view=details");
    const overview = await screen.findByTestId("ribbon-overview");
    expect(overview).toHaveAccessibleName("Overview");
    const leading = screen.getByTestId("ribbon-leading");
    const author = screen.getByTestId("ribbon-group-author");
    expect(
      leading.compareDocumentPosition(author) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(within(author).getByTestId("ribbon-action-new-doc")).toBeVisible();
    await screen.findByTestId("left-sidebar");
    expect(screen.queryByTestId("sidebar-activity-tab")).toBeNull();
    expect(screen.queryByTestId("workspace-landing")).toBeNull();
    await userEvent.click(overview);
    await screen.findByTestId("workspace-landing");
    expect(screen.getAllByTestId("center-content-tab")[0]).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Issues & tasks" }),
    );
    await screen.findByTestId("workspace-issues-section");
    expect(screen.getByRole("button", { name: "Tasks" })).toBeInTheDocument();
  });
  it("renders no title bar under alignment: workspace controls in the ribbon, global controls in the strip (g5.workbench.shell-chrome)", async () => {
    installIssues();
    // The strip renders only inside the tabs provider, as in the app.
    renderApp(
      <WorkspaceTabsProvider>
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>
      </WorkspaceTabsProvider>,
      { route: "/g5labs/test-workspace", user: createMockUser() },
    );
    const controls = await screen.findByTestId("workbench-workspace-controls");
    expect(document.querySelector('[data-surface="titlebar"]')).toBeNull();
    expect(screen.queryByTestId("workspace-toolbar")).toBeNull();
    expect(screen.queryByTestId("workspace-vm-status-strip")).toBeNull();
    expect(screen.queryByTestId("workspace-switcher-trigger")).toBeNull();
    expect(screen.queryByTestId("workspace-fork-current-button")).toBeNull();
    expect(screen.queryByTestId("top-bar-activity")).toBeNull();
    expect(screen.queryByTestId("top-bar-primary-issue")).toBeNull();
    expect(screen.queryByTestId("workspace-terminal-open")).toBeNull();
    // The ribbon's right end: owner menu, Stop (the mock VM runs), Details,
    // in the action-bar style's button shape.
    const ribbon = screen.getByTestId("action-ribbon");
    expect(within(ribbon).getByTestId("ribbon-trailing")).toContainElement(
      controls,
    );
    const owner = within(controls).getByTestId("workspace-owner-trigger");
    expect(owner).toHaveTextContent("Owner: Alex Chen");
    expect(owner).toHaveAttribute("aria-haspopup", "menu");
    // The same shape as the actions (plus a width cap on the owner name).
    expect(owner.className).toContain(
      within(ribbon).getByTestId("ribbon-action-new-doc").className,
    );
    expect(within(controls).getByTestId("workspace-vm-stop")).toHaveTextContent(
      "Stop",
    );
    expect(
      within(controls).getByTestId("workspace-info-button"),
    ).toHaveTextContent("Details");
    // Global controls ride the tab strip under this layout; none remain in
    // the page body.
    const shell = screen.getByTestId("shell-controls");
    expect(screen.getByTestId("workspace-tab-bar")).toContainElement(shell);
    expect(within(shell).getByTestId("settings-button")).toBeVisible();
    expect(within(shell).getByTestId("org-management-button")).toBeVisible();
    expect(within(shell).getByTestId("top-bar-notifications")).toBeVisible();
    expect(screen.getAllByTestId("settings-button")).toHaveLength(1);
    await userEvent.click(within(shell).getByTestId("settings-button"));
    expect(await screen.findByTestId("workspace-settings-page")).toBeVisible();
    await userEvent.click(
      within(controls).getByTestId("workspace-info-button"),
    );
    expect(await screen.findByTestId("workspace-details-panel")).toBeVisible();
  });
  it("opens the Terminal from the rail's Files header for the owner, and offers none to others", async () => {
    installIssues();
    renderWorkspace("/g5labs/test-workspace");
    const files = await screen.findByTestId("sidebar-files-tab");
    // The mock VM runs, so the owner's action is live.
    const terminal = within(files).getByRole("button", { name: "Terminal" });
    expect(terminal).toBeEnabled();
    await userEvent.click(terminal);
    expect(
      await screen.findByRole("tab", { name: /Terminal/ }),
    ).toBeInTheDocument();
  });
  it("keeps Base's title bar on a phone, where the desktop path serves only Base", async () => {
    installWorkspace({ isBase: true });
    vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
    renderWorkspace();
    await screen.findByTestId("phase-chevrons");
    expect(document.querySelector('[data-surface="titlebar"]')).not.toBeNull();
    expect(
      screen.getByTestId("workspace-switcher-trigger"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("workbench-workspace-controls")).toBeNull();
    expect(screen.queryByTestId("shell-controls")).toBeNull();
  });
  it("hides the rail Terminal action from a non-owner", async () => {
    installIssues({
      phase: "se",
      ownerUserId: "user-somebody-else",
    });
    renderWorkspace("/g5labs/test-workspace");
    const files = await screen.findByTestId("sidebar-files-tab");
    expect(
      within(files).queryByRole("button", { name: "Terminal" }),
    ).toBeNull();
  });
  it("offers Transfer ownership from the owner control and sends the chosen member", async () => {
    installIssues();
    const transfers: unknown[] = [];
    server.use(
      http.post("/api/v1/workspaces/:id/transfer", async ({ request }) => {
        transfers.push(await request.json());
        return HttpResponse.json({ data: { ok: true } });
      }),
    );
    renderWorkspace("/g5labs/test-workspace");
    const controls = await screen.findByTestId("workbench-workspace-controls");
    await userEvent.click(
      within(controls).getByTestId("workspace-owner-trigger"),
    );
    const menu = await screen.findByRole("menu", {
      name: "Transfer ownership",
    });
    const options = within(menu).getAllByTestId("workspace-owner-option");
    expect(options.length).toBeGreaterThan(0);
    expect(
      options.some(
        (option) => option.getAttribute("data-user-id") === "user-alex",
      ),
    ).toBe(false);
    await userEvent.click(options[0]);
    await userEvent.click(within(menu).getByTestId("workspace-owner-transfer"));
    await waitFor(() => expect(transfers).toHaveLength(1));
    expect(transfers[0]).toMatchObject({
      toUserId: options[0].getAttribute("data-user-id"),
    });
    expect(
      screen.queryByRole("menu", { name: "Transfer ownership" }),
    ).toBeNull();
  });
  it("the icon rail has no collapse control", async () => {
    installIssues();
    seedUserSettings({ sidebarDisplayMode: "expand-on-hover" });
    renderWorkspace("/g5labs/test-workspace");
    await screen.findByTestId("sidebar-rail");
    expect(
      screen.queryByRole("button", { name: "Collapse sidebar" }),
    ).toBeNull();
  });
  it("collapses the full sidebar from its own control and expands it from the edge; no title-bar toggle under alignment", async () => {
    installIssues();
    seedUserSettings({ sidebarDisplayMode: "expanded" });
    renderWorkspace("/g5labs/test-workspace");
    await screen.findByTestId("left-sidebar");
    expect(screen.queryByTestId("top-bar-toggle-left-sidebar")).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: "Collapse sidebar" }),
    );
    expect(screen.queryByTestId("left-sidebar")).toBeNull();
    const edge = screen.getByTestId("sidebar-collapsed-edge");
    await userEvent.click(
      within(edge).getByRole("button", { name: "Expand sidebar" }),
    );
    await screen.findByTestId("left-sidebar");
    expect(screen.queryByTestId("sidebar-collapsed-edge")).toBeNull();
  });
  // g5.ui.sidebar-default: the collapsed state of `collapse-to-rail` is the
  // icon rail with an expand control, not the bare edge; an icon press
  // expands the full sidebar on that section.
  it("collapse-to-rail collapses to the icon rail and expands from its control or a section icon", async () => {
    installIssues();
    seedUserSettings({ sidebarDisplayMode: "collapse-to-rail" });
    renderWorkspace("/g5labs/test-workspace");
    await screen.findByTestId("left-sidebar");
    await screen.findByTestId("sidebar-files-tab");
    await userEvent.click(
      screen.getByRole("button", { name: "Collapse sidebar" }),
    );
    const rail = await screen.findByTestId("sidebar-rail");
    expect(screen.queryByTestId("sidebar-collapsed-edge")).toBeNull();
    expect(screen.queryByTestId("sidebar-files-tab")).toBeNull();
    await userEvent.click(
      within(rail).getByRole("button", { name: "Expand sidebar" }),
    );
    await screen.findByTestId("sidebar-files-tab");
    expect(screen.queryByTestId("sidebar-rail")).toBeNull();

    await userEvent.click(
      screen.getByRole("button", { name: "Collapse sidebar" }),
    );
    await userEvent.click(await screen.findByTestId("sidebar-rail-docs"));
    const docsTab = await screen.findByTestId("sidebar-docs-tab");
    expect(screen.queryByTestId("sidebar-rail")).toBeNull();
    expect(
      within(docsTab).getByRole("button", { name: /docs/i }),
    ).toHaveAttribute("aria-expanded", "true");
  });
  it("Implement starts a new persona session with the issue message even when the current session has that persona", async () => {
    const { issue } = installIssues();
    const { starts, resolves } = installWorkbench({ sessionPersona: "se" });
    renderWorkspace("/g5labs/test-workspace?session=chat-tgr-1");
    await openIssue();
    await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
    const implement = screen.getByRole("menuitem", {
      name: "Implement in this workspace",
    });
    await waitFor(() => expect(implement).toBeEnabled());
    await userEvent.click(implement);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      persona: "se",
      phase: "se",
      configRevision: WORKBENCH_REVISION,
      content: startMessageFromIssue(issue),
      scopeType: "workspace",
    });
    expect(starts[0]).not.toHaveProperty("actionId");
    expect(starts[0]).not.toHaveProperty("workbenchContext");
    expect(resolves.length).toBeGreaterThan(0);
    await screen.findByTestId("chat-phase-pill");
  });
  it("Request approval opens the shared workspace approval list", async () => {
    installIssues({ phase: "ie" });
    seedPhasedApprovals();
    renderWorkspace("/g5labs/test-workspace");
    await openIssue();
    await userEvent.click(
      screen.getByRole("button", { name: "Request approval" }),
    );
    expect(
      await screen.findByTestId("workspace-approvals-outgoing"),
    ).toBeVisible();
    expect(screen.queryByTestId("workspace-approvals-phase-filter")).toBeNull();
  });
  it("Detach warns about losing the primary issue, allows cancel, then unlinks and removes actions", async () => {
    const { unlinked } = installIssues();
    renderWorkspace("/g5labs/test-workspace");
    await openIssue();
    const detach = async () => {
      await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
      await userEvent.click(
        screen.getByRole("menuitem", { name: "Detach from workspace" }),
      );
    };
    await detach();
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "This workspace will lose its primary issue.",
    );
    expect(unlinked).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(unlinked).toEqual([]);
    await detach();
    await userEvent.click(
      screen.getByRole("button", { name: "Confirm unlink" }),
    );
    await waitFor(() => expect(unlinked).toEqual(["link-1"]));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Action ▾" })).toBeNull(),
    );
    expect(
      screen.queryByRole("button", { name: "Request approval" }),
    ).toBeNull();
  });
  it.each([{ isBase: true }, { ownerUserId: "another-user" }])(
    "disables Implement for a workspace that is not writable: %j",
    async (options) => {
      installIssues({ ...options });
      installWorkbench();
      renderWorkspace("/g5labs/test-workspace");
      await openIssue();
      await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
      const implement = screen.getByRole("menuitem", {
        name: "Implement in this workspace",
      });
      expect(implement).toBeDisabled();
      expect(implement).toHaveAttribute(
        "title",
        "Chat needs a writable workspace you own",
      );
      if (options.ownerUserId) {
        expect(
          screen.queryByRole("menuitem", { name: "Detach from workspace" }),
        ).toBeNull();
        expect(
          screen.queryByRole("button", { name: "Import task" }),
        ).toBeNull();
      }
    },
  );
  it("offers Import task and quick-add disabled on Base with the Base reason while Detach stays enabled (g5.issue.base-links)", async () => {
    installIssues({ isBase: true });
    installWorkbench();
    renderWorkspace("/g5labs/test-workspace");
    const importTask = await screen.findByRole("button", {
      name: "Import task",
    });
    expect(importTask).toBeDisabled();
    expect(importTask).toHaveAttribute("title", BASE_ISSUE_LINK_REASON);
    // `openIssue` expands the rail's Issues & tasks section on its way.
    await openIssue();
    const quickAdd = screen.getByTestId("workspace-issues-quick-add");
    expect(quickAdd).toBeDisabled();
    expect(quickAdd).toHaveAttribute("title", BASE_ISSUE_LINK_REASON);
    await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
    const importItem = screen.getByRole("menuitem", { name: "Import task" });
    expect(importItem).toBeDisabled();
    expect(importItem).toHaveAttribute("title", BASE_ISSUE_LINK_REASON);
    // An existing Base link can still be detached.
    expect(
      screen.getByRole("menuitem", { name: "Detach from workspace" }),
    ).toBeEnabled();
  });
  it("opens the same import picker from the rail and the issue page", async () => {
    installIssues();
    renderWorkspace("/g5labs/test-workspace");
    await userEvent.click(
      await screen.findByRole("button", { name: "Import task" }),
    );
    await screen.findByRole("dialog", { name: "Import task" });
    await userEvent.keyboard("{Escape}");
    await openIssue();
    await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Import task" }),
    );
    await screen.findByRole("dialog", { name: "Import task" });
  });
  it("Mark as done closes the issue as the viewer's own account from the issue page", async () => {
    const { link } = installIssues();
    const bodies: Array<{ linkId: string; state: string }> = [];
    let refuse = false;
    // GitHub's state as the list re-reads it: a refusal re-reads the row.
    let current: "open" | "closed" = "open";
    serveGithubConnection(true);
    server.use(
      http.get("/api/v1/workspaces/:id/issue-links", () =>
        HttpResponse.json({
          data: {
            links: [
              {
                ...link,
                issue: link.issue && { ...link.issue, state: current },
              },
            ],
          },
        }),
      ),
      http.post(
        "/api/v1/workspaces/:id/issue-links/:linkId/state",
        async ({ request, params }) => {
          const body = (await request.json()) as { state: "closed" | "open" };
          bodies.push({ linkId: String(params.linkId), state: body.state });
          if (refuse) return issueConflictResponse("public_destination");
          current = body.state;
          return HttpResponse.json({
            data: makeIssueStateChange(body.state, link.issue ?? {}),
          });
        },
      ),
    );
    renderWorkspace("/g5labs/test-workspace");
    await openIssue();
    await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
    const done = screen.getByRole("menuitem", { name: "Mark as done" });
    await waitFor(() => expect(done).toBeEnabled());
    await userEvent.click(done);
    await waitFor(() =>
      expect(bodies).toEqual([{ linkId: link.id, state: "closed" }]),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId(`workspace-issues-done-${link.id}`),
      ).toBeChecked(),
    );
    await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
    // A refused reopen is announced and the issue stays closed.
    refuse = true;
    await userEvent.click(screen.getByRole("menuitem", { name: "Reopen" }));
    await waitFor(() => expect(bodies).toHaveLength(2));
    expect(bodies[1]).toEqual({ linkId: link.id, state: "open" });
    const toasts = await screen.findAllByTestId("toast");
    expect(
      toasts.some((toast) =>
        toast.textContent?.includes("Could not reopen acme/api#12"),
      ),
    ).toBe(true);
    expect(
      screen.getByTestId(`workspace-issues-done-${link.id}`),
    ).toBeChecked();
  });
  it("Fork opens the existing create dialog with the issue preselected", async () => {
    installIssues();
    installWorkbench();
    renderWorkspace("/g5labs/test-workspace");
    await openIssue();
    await userEvent.click(screen.getByRole("button", { name: "Action ▾" }));
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Fork in own workspace" }),
    );
    expect(await screen.findByTestId("clone-issue-ref")).toHaveValue(
      "acme/api#12",
    );
  });
});

// ---------------------------------------------------------------------------
// Guided journey (workbench g5.workbench.journey, .journey.gate,
// .journey.state, .journey.marks)
// ---------------------------------------------------------------------------

describe("WorkspacePage — guided journey (g5.workbench.journey)", () => {
  // The guided experience is off by default (g5.ui.guided-toggle); this
  // suite drives the section, so it opts in.
  beforeEach(() => seedUserSettings({ showGuidedJourney: true }));
  /** The Guided section's own data: docs with paths, marks, pull requests. */
  function installGuided(options: {
    phase?: "be" | "se" | "ie";
    ownerUserId?: string;
    features?: string[];
    docs?: Array<{
      id: string;
      name: string;
      filePath: string;
      docType?: string;
    }>;
    marks?: Array<{ stepId: string; done: boolean }>;
    pullRequests?: Array<{ id: string; status: "open" | "merged" }>;
  }) {
    const patches = installWorkspace({
      features: options.features ?? ["guided_journey"],
      ownerUserId: options.ownerUserId,
      phase: options.phase,
      detail: { docsFolderPath: "docs/base" },
    });
    installWorkbench();
    const puts: Array<{ stepId: string; done: boolean; skipped: boolean }> = [];
    let marks: JourneyMarkDTO[] = (options.marks ?? []).map((mark) => ({
      skipped: false,
      ...mark,
      markedAt: "2026-10-03T00:00:00Z",
      actorUserId: "user-alex",
    }));
    server.use(
      http.get("/api/v1/workspaces/:wsId/docs", () =>
        HttpResponse.json({
          data: (options.docs ?? []).map((doc) =>
            createMockDocNode({
              id: doc.id,
              name: doc.name,
              filePath: doc.filePath,
              docType: doc.docType ?? "doc",
            }),
          ),
        }),
      ),
      http.get("/api/v1/workspaces/:id/journey", () =>
        HttpResponse.json({ data: { marks } }),
      ),
      http.put(
        "/api/v1/workspaces/:id/journey/steps/:stepId",
        async ({ params, request }) => {
          const body = (await request.json()) as {
            done: boolean;
            skipped?: boolean;
          };
          puts.push({
            stepId: String(params.stepId),
            done: body.done,
            skipped: body.skipped ?? false,
          });
          marks = [
            ...marks.filter((mark) => mark.stepId !== String(params.stepId)),
            {
              stepId: String(params.stepId),
              done: body.done,
              skipped: body.skipped ?? false,
              markedAt: "2026-10-03T01:00:00Z",
              actorUserId: "user-alex",
            },
          ];
          return HttpResponse.json({ data: { marks } });
        },
      ),
      http.get("/api/v1/workspaces/:id/pull-requests", ({ request }) => {
        const url = new URL(request.url);
        if (url.searchParams.get("role") !== "source") {
          return HttpResponse.json({ data: [] });
        }
        return HttpResponse.json({
          data: (options.pullRequests ?? []).map((pr) => ({
            id: pr.id,
            sourceWorkspaceId: RESOLVED_WORKSPACE_ID,
            sourceWorkspaceName: "base",
            targetWorkspaceId: "parent-1",
            targetWorkspaceName: "parent",
            status: pr.status,
            localMergeStatus: "clean",
            providerStatus: "none",
            title: "Ship it",
            descriptionOverride: null,
            sourceHeads: {},
            targetHeads: {},
            preflight: { status: "clean" },
            lastPreflightFingerprint: "",
            blockingMessage: null,
            createdAt: "2026-10-02T00:00:00Z",
            updatedAt: "2026-10-02T00:00:00Z",
            mergedAt: pr.status === "merged" ? "2026-10-02T01:00:00Z" : null,
          })),
        });
      }),
    );
    return { patches, puts };
  }

  /**
   * Guided starts collapsed like every section (ui g5.ui.guided-collapsed):
   * expand it from its header and return its body.
   */
  async function openGuided(): Promise<HTMLElement> {
    const tab = await screen.findByTestId("sidebar-guided-tab");
    const header = within(tab).getByRole("button", { name: "Guided" });
    expect(header).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("guided-journey")).toBeNull();
    await userEvent.click(header);
    return screen.findByTestId("guided-journey");
  }

  it("starts collapsed after Activity and, expanded, lists the active phase's steps and buttons; hides without the feature", async () => {
    installGuided({});
    renderWorkspace();
    const journey = await openGuided();
    const tab = screen.getByTestId("sidebar-guided-tab");
    expect(journey).toBeVisible();
    // Rail order: Activity, then Guided.
    const tabs = Array.from(
      screen
        .getByTestId("left-sidebar")
        .querySelectorAll<HTMLElement>(
          '[data-testid^="sidebar-"][data-testid$="-tab"]',
        ),
    ).map((el) => el.dataset.testid);
    expect(tabs[0]).toBe("sidebar-guided-tab");
    expect(within(tab).getByRole("button", { name: "Guided" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    // The active phase lists every step; the next one is open with its
    // buttons, the others open on their label (g5.ui.guided-panel); the
    // summary names the next step's outcome.
    expect(screen.getByTestId("guided-phase-be")).toHaveAttribute(
      "data-relation",
      "active",
    );
    expect(
      screen.getByTestId("guided-step-be-intent-action-new-doc"),
    ).toBeEnabled();
    expect(
      screen.queryByTestId("guided-step-be-mock-action-build-mock"),
    ).toBeNull();
    expect(screen.getByTestId("guided-step-be-mock-toggle")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.getByTestId("guided-summary")).toHaveTextContent(
      "Capture what the business wants",
    );
  });

  it("renders no Guided section without the guided_journey feature", async () => {
    installGuided({ features: [] });
    renderWorkspace();
    await screen.findByTestId("left-sidebar");
    expect(screen.queryByTestId("sidebar-guided-tab")).toBeNull();
    expect(screen.queryByTestId("guided-journey")).toBeNull();
  });

  // g5.ui.guided-toggle: the person's switch hides the section (and loads
  // none of its data) without touching the instance feature.
  it("renders no Guided section while the guided experience setting is off", async () => {
    installGuided({});
    seedUserSettings({ showGuidedJourney: false });
    renderWorkspace();
    await screen.findByTestId("left-sidebar");
    await screen.findByTestId("action-ribbon");
    expect(screen.queryByTestId("sidebar-guided-tab")).toBeNull();
    expect(screen.queryByTestId("guided-journey")).toBeNull();
  });

  it("dispatches a step button like the ribbon: New doc opens the Create Project Doc modal", async () => {
    installGuided({});
    renderWorkspace();
    await openGuided();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId("guided-step-be-intent-action-new-doc"),
    );
    expect(await screen.findByTestId("create-doc-mode-plain")).toBeVisible();
  });

  it("completes steps from documents and marks, PUTs the owner's mark, and keeps evidence undoable only by the facts", async () => {
    const { puts } = installGuided({
      phase: "se",
      docs: [
        {
          id: "d-req",
          name: "requirements.md",
          filePath: "docs/base/requirements.md",
        },
        {
          id: "d-mock",
          name: "home.html",
          filePath: "docs/base/mocks/home.html",
        },
      ],
      marks: [{ stepId: "se-plan", done: true }],
    });
    renderWorkspace();
    await openGuided();
    const user = userEvent.setup();
    // BE is done by its documents and marks; expanded, its mock renders as
    // a chip.
    const be = await screen.findByTestId("guided-phase-be");
    expect(be).toHaveAttribute("data-relation", "done-phase");
    await user.click(within(be).getByTestId("guided-phase-be-toggle"));
    expect(within(be).getByTestId("guided-step-be-mock")).toHaveAttribute(
      "data-state",
      "done",
    );
    expect(
      within(be).getByTestId("guided-step-be-mock-artifacts"),
    ).toHaveTextContent("home.html");
    // SE: the plan is marked; the diff review is not.
    await waitFor(() =>
      expect(screen.getByTestId("guided-step-se-plan")).toHaveAttribute(
        "data-state",
        "done",
      ),
    );
    const review = screen.getByTestId("guided-step-se-diff-review");
    expect(review).toHaveAttribute("data-state", "todo");
    await user.click(screen.getByTestId("guided-step-se-diff-review-mark"));
    await waitFor(() => {
      expect(screen.getByTestId("guided-step-se-diff-review")).toHaveAttribute(
        "data-state",
        "done",
      );
    });
    expect(puts).toEqual([
      { stepId: "se-diff-review", done: true, skipped: false },
    ]);
  });

  it("gives a reader no step controls and names who runs the step", async () => {
    installGuided({ ownerUserId: "someone-else" });
    renderWorkspace();
    await openGuided();
    const intent = await screen.findByTestId("guided-step-be-intent");
    expect(
      within(intent).queryByTestId("guided-step-be-intent-mark"),
    ).toBeNull();
    expect(
      within(intent).queryByTestId("guided-step-be-intent-action-new-doc"),
    ).toBeNull();
    expect(
      within(intent).getByTestId("guided-step-be-intent-owner"),
    ).toHaveTextContent(/runs this step$/);
    expect(screen.getByTestId("guided-summary")).toHaveTextContent(
      "owns this workspace.",
    );
  });

  it("offers the phase move once every step is done and PATCHes it through the owner's move", async () => {
    const { patches } = installGuided({
      docs: [
        {
          id: "d-req",
          name: "requirements.md",
          filePath: "docs/base/requirements.md",
        },
        {
          id: "d-mock",
          name: "home.html",
          filePath: "docs/base/mocks/home.html",
        },
      ],
      marks: [{ stepId: "be-open-questions", done: true }],
    });
    renderWorkspace();
    await openGuided();
    const user = userEvent.setup();
    const move = await screen.findByTestId("guided-summary-move");
    expect(move).toHaveTextContent("Move to System Engineering");
    await user.click(move);
    await waitFor(() => {
      expect(patches).toEqual([{ phase: "se" }]);
    });
    await waitFor(() => {
      expect(screen.getByTestId("phase-chevrons")).toHaveAttribute(
        "data-phase",
        "se",
      );
    });
  });

  it("keeps the Guided phase journey independent of approval rosters", async () => {
    installGuided({});
    seedPhasedApprovals();
    renderWorkspace();
    await openGuided();
    expect(screen.queryByTestId(/^guided-gate-/)).toBeNull();
    await userEvent.click(screen.getByTestId("ribbon-workspace-approvals"));
    expect(
      await screen.findByTestId("workspace-approvals-outgoing"),
    ).toBeVisible();
  });
});

it("binds a private file opened before VM discovery and withdraws it on stop", async () => {
  installWorkspace({});
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const vm = createMockWorkspaceVm({
    workspaceId: RESOLVED_WORKSPACE_ID,
    ownerUserId: "user-alex",
    status: "running",
    vmId: "late-discovered-vm",
  });
  server.use(
    http.get("/api/v1/workspaces/:id/vm", async () => {
      await held;
      return HttpResponse.json({ data: vm });
    }),
    http.get("/api/v1/workspaces/:id/files/*", () =>
      HttpResponse.json({
        data: {
          path: "early-private.txt",
          content: "PRIVATE EARLY BYTES",
          size: 19,
          encoding: "utf-8",
          hash: "early",
          source: "live",
          repoOid: null,
          snapshotAt: null,
        },
      }),
    ),
  );
  const { queryClient } = renderWorkspace(
    "/org-g5labs/test-workspace/file/early-private.txt",
  );
  await screen.findByText("Checking current file access…");
  expect(screen.queryByTestId("code-editor")).toBeNull();
  await act(async () => {
    release();
    await held;
  });
  await screen.findByTestId("code-editor");
  await act(async () => {
    queryClient.setQueryData(["workspace-vm", RESOLVED_WORKSPACE_ID], {
      ...vm,
      status: "stopped",
    });
  });
  await waitFor(() => {
    expect(screen.queryByRole("tab", { name: "early-private.txt" })).toBeNull();
    expect(screen.queryByTestId("code-editor")).toBeNull();
  });
});
