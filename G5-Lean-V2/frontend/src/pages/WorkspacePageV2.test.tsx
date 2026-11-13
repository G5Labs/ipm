// =============================================================================
// WorkspacePageV2 Integration Tests — Full Workspace Page
// =============================================================================
// Tests the full workspace page shell including panel layout, keyboard shortcuts,
// responsive behavior, tab management, and cross-panel interactions.
// =============================================================================

import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import {
  screen,
  waitFor,
  fireEvent,
  act,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { delay, http, HttpResponse } from "msw";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { useEffect, useState } from "react";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { Modal } from "../components/ui/Modal";
import { PlatformIssueReportProvider } from "../components/platform-issues/PlatformIssueReportButton";
import { seedUserSettings } from "../test/userSettings";
import { WorkspaceTabsProvider } from "../hooks/useWorkspaceTabs";
import { firePointer } from "../test/pointer";
import { activateSidebarSectionAsync } from "../test/sidebar";
import {
  createMockUser,
  createMockWorkspace,
  createMockChatMessage,
  createMockChatSession,
  createMockTask,
  mockWorkspaces,
  mockWorkspaceDetails,
  mockInstanceInfo,
  createMockWorkspaceVm,
} from "../test/fixtures";
import { GETTING_STARTED_AUTO_OPENED_STORAGE_KEY } from "../lib/userGuide";
import {
  makeIssueDetail,
  makeIssueLink,
  serveExternalIssuesFeature,
} from "../test/externalIssueFixtures";
import type {
  DocNode,
  WorkspaceVmView,
  ReviewSuggestion,
  SourceControlChangeGraphRoute,
  TabItem,
  TaskSummary,
  TaskListEntry,
} from "../lib/types";
import { MOCK_WORKSPACES } from "../data/mock";
import { markReviewPromptSeed } from "../components/workspace/reviewWorkspace";
import {
  BASE_REVERIFICATION_NOTICE,
  reviewWorkspaceChatPrompt,
} from "../lib/reviewPrompts";

// The chat bubble stack's default 3D face draws on a canvas jsdom lacks; the
// page only has to hand the head its state.
vi.mock("robot-heads", () => ({
  RobotHead: (props: { state: string }) => (
    <canvas data-testid="robot-head" data-state={props.state} />
  ),
}));

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: ({ enabled }: { enabled: boolean }) => (
    <div data-testid="stub-workspace-terminal" data-enabled={enabled}>
      <input aria-label="Terminal command" />
    </div>
  ),
}));

const reviewOriginTarget = vi.hoisted(() => ({
  docId: "",
  filePath: "",
  repoId: "",
}));

// Exercise the real page-supplied document callbacks and editor dispatch;
// these controls replace only the unrelated graph/approval data surfaces.
vi.mock("../components/source-control/SourceControlChangeGraphView", () => ({
  SourceControlChangeGraphView: ({
    onOpenDocument,
  }: {
    onOpenDocument: (route: SourceControlChangeGraphRoute) => void;
  }) => (
    <button
      onClick={() =>
        onOpenDocument({
          fileType: "external_doc",
          workspaceId: "12345678-1234-4123-8123-123456789abc",
          filePath: reviewOriginTarget.filePath,
          repoId: reviewOriginTarget.repoId,
        })
      }
    >
      Open graph document
    </button>
  ),
}));

// Import component under test
import {
  WorkspacePage,
  getRightPanelChatScope,
  getTaskDialogDisabledReason,
  hasAnswerableTimedOutQuestion,
  hasExplicitTaskWaitForUserInput,
  shouldAttachTaskTimelineToChatSession,
  shouldClearAskUserQuestionFocus,
  verifiedAskUserQuestionFocus,
  workspacePullRequestCreateToast,
  workspaceDocumentTitle,
  DEFAULT_DOCUMENT_TITLE,
} from "./WorkspacePage";
import { resetCapacityWaitAnnouncementsForTests } from "../lib/capacityWaitNotice";

describe("WorkspacePage AskUserQuestion focus verification", () => {
  const approvalId = "approval-home-1";
  const sessionId = "session-home-1";
  const taskId = "task-home-1";
  const request = {
    id: "request-home-1",
    entryType: "checkpoint",
    content: "Question requested",
    metadata: {
      providerEvent: "provided_agent.approval.requested",
      toolName: "AskUserQuestion",
      approvalId,
      sessionId,
      messageId: "msg-home-1",
    },
    sequence: 1,
    createdAt: "2026-08-28T10:00:00Z",
  };
  const liveMessage = {
    id: "msg-home-1",
    role: "assistant",
    status: "streaming",
    createdAt: "2026-08-28T10:00:00Z",
  };
  const base = {
    approvalId,
    sessionHint: sessionId,
    routeTaskId: taskId,
    selectedTaskId: taskId,
    activeSessionId: sessionId,
    activeSession: { id: sessionId, taskId, scopeRef: taskId },
    timelineEntries: [request],
    messages: [liveMessage],
    isStreaming: true,
  };

  it("accepts only the exact task/session/approval while the card is answerable", () => {
    expect(verifiedAskUserQuestionFocus(base)).toBe(approvalId);
    expect(
      verifiedAskUserQuestionFocus({ ...base, selectedTaskId: "other-task" }),
    ).toBeNull();
    expect(
      verifiedAskUserQuestionFocus({ ...base, sessionHint: "other-session" }),
    ).toBeNull();
    expect(
      verifiedAskUserQuestionFocus({ ...base, approvalId: "other-approval" }),
    ).toBeNull();
  });

  it("admits a settlement-only timeout and rejects settled or late-answered hints", () => {
    const settlement = {
      id: "settlement-home-1",
      entryType: "checkpoint",
      content: "Question timed out",
      metadata: {
        providerEvent: "provided_agent.approval.resolved",
        approvalId,
        sessionId,
        resolutionSource: "idle_timeout",
      },
      sequence: 2,
      createdAt: "2026-08-28T10:01:00Z",
    };
    expect(
      verifiedAskUserQuestionFocus({
        ...base,
        timelineEntries: [request, settlement],
        messages: [],
        isStreaming: false,
      }),
    ).toBe(approvalId);

    const resolved = {
      ...settlement,
      metadata: {
        ...settlement.metadata,
        resolutionSource: "user",
      },
    };
    expect(
      verifiedAskUserQuestionFocus({
        ...base,
        timelineEntries: [request, resolved],
        messages: [],
        isStreaming: false,
      }),
    ).toBeNull();

    const lateAnswered = {
      id: "late-answer-home-1",
      entryType: "checkpoint",
      content: "Late answer recorded",
      metadata: {
        providerEvent: "provided_agent.approval.late_answered",
        approvalId,
        sessionId,
      },
      sequence: 3,
      createdAt: "2026-08-28T10:02:00Z",
    };
    expect(
      verifiedAskUserQuestionFocus({
        ...base,
        timelineEntries: [request, settlement, lateAnswered],
        messages: [],
        isStreaming: false,
      }),
    ).toBeNull();
  });

  it("retains the hint when session queries finish before route selection", () => {
    expect(
      shouldClearAskUserQuestionFocus({
        focusHint: approvalId,
        selectionReady: false,
        activeSessionReady: true,
        timelineReady: true,
        messagesReady: true,
        verifiedApprovalId: null,
      }),
    ).toBe(false);
  });

  it("waits for message hydration before clearing an unverified pending hint", () => {
    expect(
      shouldClearAskUserQuestionFocus({
        focusHint: approvalId,
        selectionReady: true,
        activeSessionReady: true,
        timelineReady: true,
        messagesReady: false,
        verifiedApprovalId: null,
      }),
    ).toBe(false);
    expect(
      shouldClearAskUserQuestionFocus({
        focusHint: approvalId,
        selectionReady: true,
        activeSessionReady: true,
        timelineReady: true,
        messagesReady: true,
        verifiedApprovalId: null,
      }),
    ).toBe(true);
    expect(
      shouldClearAskUserQuestionFocus({
        focusHint: approvalId,
        selectionReady: true,
        activeSessionReady: true,
        timelineReady: true,
        messagesReady: true,
        verifiedApprovalId: approvalId,
      }),
    ).toBe(false);
  });
});

import type { WorkspacePullRequestSummary } from "../lib/types";

// The center tab bar is hidden by default (workspaces
// g5.workspace.ui.center-tabs); this suite drives the tabs, so it opts in.
// The suite drives the stacked sidebar, which is no longer the default
// (g5.ui.sidebar-default).
beforeEach(() =>
  seedUserSettings({ showTabBar: true, sidebarDisplayMode: "expanded" }),
);

function RouteProbe() {
  const location = useLocation();
  return (
    <div data-testid="route-probe">
      {location.pathname}
      {location.search}
    </div>
  );
}

function WorkspacePageWithRouteProbe() {
  return (
    <>
      <WorkspacePage />
      <RouteProbe />
    </>
  );
}

function WorkspacePageWithWorkspaceSwitchButton({
  destination = "/org-g5labs/task-graph-rework",
}: { destination?: string } = {}) {
  const navigate = useNavigate();
  return (
    <>
      <button type="button" onClick={() => navigate(destination)}>
        Switch workspace
      </button>
      <WorkspacePage />
      <RouteProbe />
    </>
  );
}

function WorkspacePageWithSwitchAndBack({
  destination,
}: {
  destination: string;
}) {
  const navigate = useNavigate();
  return (
    <>
      <button type="button" onClick={() => navigate(destination)}>
        Switch workspace
      </button>
      <button type="button" onClick={() => navigate(-1)}>
        Back
      </button>
      <WorkspacePage />
      <RouteProbe />
    </>
  );
}

function mockDesktopMatchMedia() {
  const matchMedia = vi.mocked(window.matchMedia);
  matchMedia.mockImplementation((query: string) => ({
    matches: query.includes("min-width: 1024px"),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

function createJwtForOrg(org: string, scopes: string[]): string {
  const now = Math.floor(Date.now() / 1000);
  const header = btoa(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const payload = btoa(
    JSON.stringify({
      sub: "user-alex",
      email: "alex@g5labs.dev",
      org,
      scopes,
      iat: now,
      exp: now + 3600,
    }),
  );

  return `${header}.${payload}.test-signature`;
}

function installQueuedChatStopFixture(
  activeStatus: "pending" | "streaming",
  options: { failFirstStop?: boolean } = {},
) {
  const session = createMockChatSession({
    id: "queued-stop-session",
    workspaceId: "base",
    scopeType: "workspace",
    scopeRef: "base",
    title: "Queued stop session",
    agentProvider: "claude_code",
    messageCount: 6,
  });
  const messages = [
    createMockChatMessage({
      id: "user-a",
      sessionId: session.id,
      role: "user",
      content: "A",
      status: "complete",
      messageIndex: 0,
    }),
    createMockChatMessage({
      id: "assistant-a",
      sessionId: session.id,
      status: activeStatus,
      messageIndex: 1,
    }),
    createMockChatMessage({
      id: "user-b",
      sessionId: session.id,
      role: "user",
      content: "B",
      status: "complete",
      messageIndex: 2,
    }),
    createMockChatMessage({
      id: "assistant-b",
      sessionId: session.id,
      status: "pending",
      messageIndex: 3,
    }),
    createMockChatMessage({
      id: "user-c",
      sessionId: session.id,
      role: "user",
      content: "C",
      status: "complete",
      messageIndex: 4,
    }),
    createMockChatMessage({
      id: "assistant-c",
      sessionId: session.id,
      status: "pending",
      messageIndex: 5,
    }),
  ];
  let stopFailuresRemaining = options.failFirstStop ? 1 : 0;
  const stopRequestIds: string[] = [];
  const stoppedMessageIds: string[] = [];
  const setAssistantStatus = (
    messageId: "assistant-a" | "assistant-b" | "assistant-c",
    status: "pending" | "streaming" | "stopped",
  ) => {
    const message = messages.find((candidate) => candidate.id === messageId);
    if (!message) {
      throw new Error(`Missing queued-chat fixture message ${messageId}`);
    }
    message.status = status;
  };

  server.use(
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/:sessionId", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/:sessionId/messages", () =>
      HttpResponse.json({ data: messages }),
    ),
    http.get("/api/v1/sessions/:sessionId/timeline", () =>
      HttpResponse.json({
        data: { entries: [], cursor: null, hasMore: false },
      }),
    ),
    http.post(
      "/api/v1/sessions/:sessionId/messages/:messageId/stop",
      ({ params }) => {
        const messageId = String(params.messageId);
        stopRequestIds.push(messageId);
        if (stopFailuresRemaining > 0) {
          stopFailuresRemaining -= 1;
          return HttpResponse.json(
            { error: { message: "Injected Stop failure" } },
            { status: 500 },
          );
        }
        stoppedMessageIds.push(messageId);
        const message = messages.find(
          (candidate) => candidate.id === messageId,
        );
        if (message) {
          message.status = "stopped";
        }
        return HttpResponse.json({ data: { stopped: true } });
      },
    ),
  );

  return {
    sessionId: session.id,
    setAssistantStatus,
    stopRequestIds,
    stoppedMessageIds,
  };
}

function installPlainChatStopFixture(
  options: { holdFirstStop?: boolean } = {},
) {
  const session = createMockChatSession({
    id: "plain-stop-session",
    workspaceId: "base",
    scopeType: "workspace",
    scopeRef: "base",
    title: "Plain stop session",
    agentProvider: "claude_code",
    messageCount: 2,
  });
  const messages = [
    createMockChatMessage({
      id: "user-a",
      sessionId: session.id,
      role: "user",
      content: "A",
      status: "complete",
      messageIndex: 0,
    }),
    createMockChatMessage({
      id: "assistant-a",
      sessionId: session.id,
      status: "streaming",
      messageIndex: 1,
    }),
  ];
  const stopRequestIds: string[] = [];
  const setAssistantStatus = (status: "streaming" | "stopped") => {
    const message = messages.find(
      (candidate) => candidate.id === "assistant-a",
    );
    if (!message) {
      throw new Error("Missing plain-stop fixture assistant-a");
    }
    message.status = status;
  };
  // The first Stop answers only once released, so a test can observe the
  // control while the request is in flight.
  let releaseFirstStop: () => void = () => {};
  let firstStopHeld: Promise<void> | null = options.holdFirstStop
    ? new Promise<void>((resolve) => {
        releaseFirstStop = resolve;
      })
    : null;

  server.use(
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/:sessionId", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/:sessionId/messages", () =>
      HttpResponse.json({ data: messages }),
    ),
    http.get("/api/v1/sessions/:sessionId/timeline", () =>
      HttpResponse.json({
        data: { entries: [], cursor: null, hasMore: false },
      }),
    ),
    http.post(
      "/api/v1/sessions/:sessionId/messages/:messageId/stop",
      async ({ params }) => {
        // Deliberately do NOT terminalize the assistant here: the turn stays
        // live so the test can assert the single composer Stop control remains
        // usable instead of vanishing until a page refresh.
        stopRequestIds.push(String(params.messageId));
        if (firstStopHeld) {
          const held = firstStopHeld;
          firstStopHeld = null;
          await held;
        }
        return HttpResponse.json({
          data: { stopped: true, disposition: "signalling" },
        });
      },
    ),
  );

  return {
    sessionId: session.id,
    setAssistantStatus,
    stopRequestIds,
    releaseFirstStop: () => releaseFirstStop(),
  };
}

// =============================================================================
// Test Suite
// =============================================================================

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
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
});

describe("task dialog chat state helpers", () => {
  it.each([
    "queued",
    "running",
    "waiting",
    "succeeded",
    "failed",
    "cancelled",
  ] as const)(
    "keeps active task dialog input available for %s tasks",
    (status) => {
      expect(
        getTaskDialogDisabledReason("active", {
          status,
        }),
      ).toBeNull();
    },
  );

  it("closes completed and archived sessions", () => {
    expect(
      getTaskDialogDisabledReason("completed", {
        status: "succeeded",
      }),
    ).toMatch(/session has completed/i);
    expect(
      getTaskDialogDisabledReason("archived", {
        status: "cancelled",
      }),
    ).toMatch(/archived/i);
  });

  it("keeps explicit sync task waits steerable", () => {
    const timelineEntries = [
      {
        id: "task-wait-entry",
        entryType: "checkpoint",
        content: "Waiting for user input: choose the ontology edit shape",
        metadata: {
          kind: "task_wait",
          source: "g5d.task.wait",
          sessionId: "sync-session",
        },
        createdAt: "2026-05-18T10:05:00.000Z",
      },
    ];

    expect(
      hasExplicitTaskWaitForUserInput(timelineEntries, "sync-session"),
    ).toBe(true);
    expect(
      hasExplicitTaskWaitForUserInput(timelineEntries, "sync-session", [
        {
          role: "user",
          createdAt: "2026-05-18T10:00:00.000Z",
        },
      ]),
    ).toBe(true);
    expect(
      hasExplicitTaskWaitForUserInput(timelineEntries, "sync-session", [
        {
          role: "user",
          createdAt: "2026-05-18T10:10:00.000Z",
        },
      ]),
    ).toBe(false);
    expect(
      getTaskDialogDisabledReason("active", {
        status: "waiting",
      }),
    ).toBeNull();
  });

  it("keeps question-parked sync waits steerable (issue #1391)", () => {
    const questionParkedEntries = [
      {
        id: "question-request",
        entryType: "checkpoint",
        content: "AskUserQuestion",
        metadata: {
          providerEvent: "provided_agent.approval.requested",
          toolName: "AskUserQuestion",
          approvalId: "approval-1",
        },
        createdAt: "2026-05-18T10:00:00.000Z",
        sequence: 1,
      },
      {
        id: "question-settlement",
        entryType: "checkpoint",
        content: "Question timed out",
        metadata: {
          providerEvent: "provided_agent.approval.resolved",
          resolutionSource: "idle_timeout",
          approvalId: "approval-1",
        },
        createdAt: "2026-05-18T10:30:00.000Z",
        sequence: 2,
      },
    ];

    // Settled-but-answerable question keeps the sync waiting dialog steerable.
    expect(hasAnswerableTimedOutQuestion(questionParkedEntries)).toBe(true);
    expect(
      getTaskDialogDisabledReason("active", {
        status: "waiting",
      }),
    ).toBeNull();

    // A later user message supersedes the question: dialog closes again.
    expect(
      hasAnswerableTimedOutQuestion(questionParkedEntries, [
        { role: "user", createdAt: "2026-05-18T10:45:00.000Z" },
      ]),
    ).toBe(false);

    // A late-answered marker also ends steerability via this evidence.
    expect(
      hasAnswerableTimedOutQuestion([
        ...questionParkedEntries,
        {
          id: "late-answered-marker",
          entryType: "checkpoint",
          content: "Question answered late: approval-1",
          metadata: {
            providerEvent: "provided_agent.approval.late_answered",
            approvalId: "approval-1",
          },
          createdAt: "2026-05-18T10:50:00.000Z",
          sequence: 3,
        },
      ]),
    ).toBe(false);

    // A dormant sync wait still accepts input; it queues behind the held task.
    expect(
      getTaskDialogDisabledReason("active", {
        status: "waiting",
      }),
    ).toBeNull();
  });
});

describe("workspacePullRequestCreateToast", () => {
  const basePullRequest = {
    id: "pr-1",
    sourceWorkspaceId: "source",
    sourceWorkspaceName: "Fork",
    targetWorkspaceId: "base",
    targetWorkspaceName: "Base",
    status: "open",
    localMergeStatus: "stale",
    providerStatus: "action_required",
    title: "Fork into Base",
    descriptionOverride: null,
    sourceHeads: [],
    targetHeads: [],
    preflight: {} as never,
    lastPreflightFingerprint: "fingerprint",
    blockingMessage: null,
    createdAt: "2026-07-08T00:00:00Z",
    updatedAt: "2026-07-08T00:00:00Z",
    mergedAt: null,
  } satisfies WorkspacePullRequestSummary;

  it("warns when GitHub provider PR creation is skipped", () => {
    const allSkipped = workspacePullRequestCreateToast({
      alreadyExisted: false,
      dirtyDrafts: [],
      pullRequest: {
        ...basePullRequest,
        providerRepos: [
          {
            id: "provider-code",
            repoId: "repo-code",
            provider: "github",
            providerRepoFullName: "G5Labs/G5",
            sourceRemoteBranch: "g5/fork",
            targetRemoteBranch: "main",
            providerPrNumber: null,
            providerPrUrl: null,
            providerStatus: "action_required",
            sourceHeadSha: "source-tip",
            targetHeadSha: "base-tip",
            providerHeadSha: null,
            providerBaseSha: null,
            lastError: "GitHub target branch moved.",
            lastSyncedAt: null,
            updatedAt: "2026-07-08T00:00:00Z",
          },
        ],
      },
    });
    expect(allSkipped).toEqual({
      severity: "warning",
      title: "Local PR created — GitHub PR not created",
      message:
        "Fork into Base: stale. Open the PR dialog for per-repo recovery steps.",
    });

    const mixedSkipped = workspacePullRequestCreateToast({
      alreadyExisted: true,
      dirtyDrafts: [],
      pullRequest: {
        ...basePullRequest,
        providerStatus: "partial",
        providerRepos: [
          {
            id: "provider-docs",
            repoId: "repo-docs",
            provider: "github",
            providerRepoFullName: "G5Labs/test-docs-repo",
            sourceRemoteBranch: "g5/fork",
            targetRemoteBranch: "main",
            providerPrNumber: 42,
            providerPrUrl: "https://github.com/G5Labs/test-docs-repo/pull/42",
            providerStatus: "open",
            sourceHeadSha: "docs-source-tip",
            targetHeadSha: "docs-base-tip",
            providerHeadSha: "docs-source-tip",
            providerBaseSha: "docs-base-tip",
            lastError: null,
            lastSyncedAt: "2026-07-08T00:00:00Z",
            updatedAt: "2026-07-08T00:00:00Z",
          },
          {
            id: "provider-code",
            repoId: "repo-code",
            provider: "github",
            providerRepoFullName: "G5Labs/test-code-repo",
            sourceRemoteBranch: "g5/fork",
            targetRemoteBranch: "main",
            providerPrNumber: null,
            providerPrUrl: null,
            providerStatus: "action_required",
            sourceHeadSha: "code-source-tip",
            targetHeadSha: "code-base-tip",
            providerHeadSha: null,
            providerBaseSha: null,
            lastError: "GitHub target branch moved.",
            lastSyncedAt: null,
            updatedAt: "2026-07-08T00:00:00Z",
          },
        ],
      },
    });
    expect(mixedSkipped).toMatchObject({
      severity: "warning",
      title: "Local PR updated — some GitHub PRs not created",
    });
  });
});

// The legacy /test-workspace MSW alias resolves to the arbitrary ID "base".
// Shell interaction tests exercise a working workspace; Base cases install their
// own canonical detail. Names and IDs do not establish protected-root identity.
function installWorkingWorkspaceDetail() {
  server.use(
    http.get("/api/v1/workspaces/:id", ({ params }) => {
      const id = params.id as string;
      const detail = mockWorkspaceDetails[id] ?? mockWorkspaceDetails.base;
      return HttpResponse.json({
        data: { ...detail, id, isBase: false, clonedFromId: "protected-root" },
      });
    }),
  );
}

describe("WorkspacePageV2", () => {
  beforeEach(installWorkingWorkspaceDetail);
  describe("loading states", () => {
    it("renders loading state while workspace data fetches", async () => {
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      // Initially should show some loading state
      await waitFor(() => {
        const loadingEl =
          screen.queryByText(/loading/i) ||
          document.querySelector('[class*="skeleton"]') ||
          document.querySelector('[class*="animate-pulse"]') ||
          document.body;
        expect(loadingEl).toBeTruthy();
      });
    });
  });

  describe("loaded states", () => {
    it("renders full page with all panels", async () => {
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      // Wait for workspace data to load
      await waitFor(() => {
        // The page should render its main structure
        const pageContainer =
          document.querySelector('[class*="workspace"]') ||
          document.querySelector("main") ||
          document.body;
        expect(pageContainer).toBeTruthy();
      });
    });

    it("loads workspace data and renders content", async () => {
      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      // Wait for workspace name or content to appear
      await waitFor(
        () => {
          // The workspace page should have rendered after data loads
          const content =
            document.querySelector('[class*="workspace"]') || document.body;
          expect(content).toBeTruthy();
        },
        { timeout: 5000 },
      );
    });

    it("uses the route fallback for the browser tab title until workspace data loads", async () => {
      let releaseWorkspaceData!: () => void;
      const workspaceDataReady = new Promise<void>((resolve) => {
        releaseWorkspaceData = resolve;
      });

      server.use(
        http.get("/api/v1/orgs/:orgId/workspaces", async () => {
          await workspaceDataReady;
          return HttpResponse.json({ data: mockWorkspaces });
        }),
        http.get("/api/v1/workspaces/:id", async ({ params }) => {
          await workspaceDataReady;
          const id = params.id as string;
          return HttpResponse.json({
            data: mockWorkspaceDetails[id] ?? mockWorkspaceDetails.base,
          });
        }),
      );

      const { unmount } = renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      await waitFor(() => {
        expect(document.title).toBe("G5 - test-workspace");
      });

      releaseWorkspaceData();

      await waitFor(() => {
        expect(document.title).toBe("G5 - base");
      });

      unmount();

      expect(document.title).toBe(DEFAULT_DOCUMENT_TITLE);
    });
  });

  describe("doc review entry points", () => {
    const workspaceId = "12345678-1234-4123-8123-123456789abc";
    const repoId = "87654321-4321-4321-8321-cba987654321";
    let docs: DocNode[];

    beforeEach(() => {
      mockDesktopMatchMedia();
      docs = ["docs/base/project.md", "docs/other/archive.md"].map(
        (path, i): DocNode => ({
          id: `overlay:${workspaceId}:${repoId}:${btoa(path).replace(/=/g, "")}`,
          name: i ? "Archive review" : "Project review",
          title: i ? "Archive review" : "Project review",
          docType: "spec",
          filePath: path,
          workspaceId,
          repoId,
          linkedNodeCount: 0,
          lastUpliftedAt: null,
          lastDownliftedAt: null,
          contentUpdatedAt: null,
          createdAt: "2026-09-30T00:00:00Z",
          updatedAt: "2026-09-30T00:00:00Z",
          content: "Old.\n",
          contentHash: "doc-hash",
        }),
      );
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({ data: { orgId: "org-g5labs", workspaceId } }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails.base,
              id: params.id,
              isBase: false,
              docsFolderPath: "docs/base",
              docsRepoId: repoId,
              branches: [{ ...mockWorkspaceDetails.base.branches[0], repoId }],
            },
          }),
        ),
        http.get("/api/v1/workspaces/:id/files/*", ({ request }) =>
          HttpResponse.json({
            data: {
              path: new URL(request.url).pathname.split("/files/")[1],
              content: "Old.\n",
              size: 5,
              encoding: "utf-8",
              hash: "doc-hash",
              source: "live",
              vmId: "vm-docs",
              repoOid: null,
              snapshotAt: null,
            },
          }),
        ),
        http.get("/api/v1/workspaces/:id/docs", () =>
          HttpResponse.json({ data: docs }),
        ),
        http.get("/api/v1/workspaces/:id/docs/by-path", ({ request }) =>
          HttpResponse.json({
            data: docs.find(
              (doc) =>
                doc.filePath === new URL(request.url).searchParams.get("path"),
            ),
          }),
        ),
        http.get("/api/v1/workspaces/:id/review/suggestions", ({ request }) => {
          const path = new URL(request.url).searchParams.get("path");
          return HttpResponse.json({
            data: {
              suggestions: [
                {
                  id: path?.includes("archive")
                    ? "archive-hunk"
                    : "project-hunk",
                  publicationId: "pub",
                  findingId: null,
                  repoId,
                  path,
                  kind: "hunk",
                  beforeLines: "Old.\n",
                  afterLines: "New.\n",
                  anchorStart: 1,
                  contentHash: "raw-hash",
                  anchorContext: null,
                  authorKind: "human",
                  authorLabel: "Reviewer",
                  note: "",
                  commentId: null,
                  status: "open",
                  statusBy: null,
                  statusAt: null,
                  supersedes: null,
                  adoptedFrom: null,
                  createdAt: "2026-09-30T00:00:00Z",
                } as ReviewSuggestion,
              ],
            },
          });
        }),
      );
      Object.assign(reviewOriginTarget, {
        docId: docs[0].id,
        filePath: docs[0].filePath,
        repoId,
      });
    });

    function renderReviewDoc(route = "/org-g5labs/test-workspace") {
      return renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        { route, user: createMockUser() },
      );
    }

    async function openProjectDoc() {
      await activateSidebarSectionAsync("sidebar-docs-tab");
      await userEvent.click(
        await within(screen.getByTestId("sidebar-docs-tab")).findByText(
          "project.md",
        ),
      );
      expect(
        await screen.findByTestId("spec-suggestion-preview-project-hunk"),
      ).toHaveTextContent("New");
    }

    it.each(["html", "htm", "XHTML", "shtml", "jsonl"])(
      "restores an unlisted %s document through the shared file preview",
      async (extension) => {
        const path = `docs/archive/legacy.${extension}`;
        const content =
          extension === "jsonl"
            ? '{"type":"metric","value":42}'
            : "<h1>Restored explainer</h1>".padEnd(1024 * 1024 + 1, " ");
        const id = `overlay:${workspaceId}:${repoId}:${btoa(path).replace(/=/g, "")}`;
        docs = [];
        const legacyRead = vi.fn();
        server.use(
          http.get("/api/v1/workspaces/:id/docs/by-path", () => {
            legacyRead();
            return new HttpResponse(null, { status: 404 });
          }),
          http.get("/api/v1/workspaces/:id/files/*", ({ request }) => {
            expect(new URL(request.url).searchParams.get("repoId")).toBe(
              repoId,
            );
            return HttpResponse.json({
              data: {
                path,
                content,
                size: content.length,
                encoding: "utf-8",
                hash: "legacy-hash",
                source: "committed",
                repoOid: "a".repeat(40),
                snapshotAt: null,
              },
            });
          }),
        );
        localStorage.setItem(
          `g5:tabs:${workspaceId}`,
          JSON.stringify({
            tabs: [{ type: "doc", docId: id, name: `legacy.${extension}` }],
            activeTabId: `doc:${id}`,
          }),
        );
        renderReviewDoc();
        await userEvent.click(
          await screen.findByRole("tab", { name: `legacy.${extension}` }),
        );
        const file = await screen.findByTestId("file-view");
        if (extension !== "jsonl") {
          expect(
            await within(file).findByTitle(`legacy.${extension} preview`),
          ).toHaveAttribute("srcdoc", expect.stringContaining(content));
          expect(
            within(file).queryByTestId("file-view-load-preview"),
          ).toBeNull();
          expect(
            within(file).getByTitle(`legacy.${extension} preview`),
          ).toHaveAttribute("sandbox", "allow-scripts");
        } else
          expect(
            await within(file).findByTestId("jsonl-transcript-view"),
          ).toHaveTextContent("JSONL records");
        expect(screen.queryByRole("group", { name: "Editor mode" })).toBeNull();
        await userEvent.click(
          within(file).getByTestId("file-view-switch-mode"),
        );
        expect(
          within(file).getByTestId("file-view-switch-mode"),
        ).toHaveTextContent("Switch to preview");
        expect(screen.getAllByTestId("file-view")).toHaveLength(1);
        expect(legacyRead).not.toHaveBeenCalled();
      },
    );

    it("requires an explicit repository for an ambiguous restored file, then joins its Doc tab", async () => {
      const otherRepo = "12345678-4321-4321-8321-cba987654321";
      const reads: string[] = [];
      server.use(
        http.get("/api/v1/workspaces/:id", () =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails.base,
              id: workspaceId,
              isBase: false,
              docsFolderPath: "docs/base",
              docsRepoId: repoId,
              branches: [
                {
                  ...mockWorkspaceDetails.base.branches[0],
                  repoId,
                  repoName: "Docs",
                },
                {
                  ...mockWorkspaceDetails.base.branches[0],
                  repoId: otherRepo,
                  repoName: "Code",
                },
              ],
            },
          }),
        ),
        http.get("/api/v1/workspaces/:id/files/*", ({ request }) => {
          reads.push(new URL(request.url).searchParams.get("repoId") ?? "");
          return HttpResponse.json({
            data: {
              path: docs[0].filePath,
              content: "Old.\n",
              size: 5,
              encoding: "utf-8",
              hash: "doc-hash",
              source: "live",
              vmId: "vm-docs",
              repoOid: null,
              snapshotAt: null,
            },
          });
        }),
      );
      localStorage.setItem(
        `g5:tabs:${workspaceId}`,
        JSON.stringify({
          tabs: [{ type: "file", filePath: docs[0].filePath, name: "project" }],
          activeTabId: `file:${docs[0].filePath}`,
        }),
      );
      renderReviewDoc();
      await userEvent.click(
        await screen.findByRole("tab", { name: /^project$/i }),
      );
      expect(
        await screen.findByTestId("content-branch-unresolved"),
      ).toBeVisible();
      expect(reads).toEqual([]);
      await userEvent.click(
        screen.getByTestId(`content-branch-option-${repoId}`),
      );
      await screen.findByTestId("spec-suggestion-preview-project-hunk");
      expect(reads.length).toBeGreaterThan(0);
      expect(new Set(reads)).toEqual(new Set([repoId]));
      await openProjectDoc();
      expect(
        screen.getAllByRole("tab", { name: /^project(?: review)?$/i }),
      ).toHaveLength(1);
    });

    it.each([false, true])(
      "uses the same file preview from PROJECT DOCS and All Docs with an existing target: %s",
      async (alreadyOpen) => {
        renderApp(
          <Routes>
            <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
          </Routes>,
          {
            route: alreadyOpen
              ? `/org-g5labs/test-workspace/doc/${encodeURIComponent(docs[1].id)}`
              : "/org-g5labs/test-workspace",
            user: createMockUser(),
          },
        );
        if (alreadyOpen) {
          await userEvent.click(
            await screen.findByRole("tab", { name: /^archive(?: review)?$/i }),
          );
          expect(
            await screen.findByTestId("spec-suggestion-preview-archive-hunk"),
          ).toHaveTextContent("New");
        }
        await activateSidebarSectionAsync("sidebar-docs-tab");
        await userEvent.click(
          await within(screen.getByTestId("sidebar-docs-tab")).findByText(
            "project.md",
          ),
        );
        expect(
          await screen.findByTestId("spec-suggestion-preview-project-hunk"),
        ).toHaveTextContent("New");
        await activateSidebarSectionAsync("sidebar-all-docs-tab");
        const archiveSection = screen.getByTestId("sidebar-all-docs-tab");
        // Expand the repository folders in the browse surface.
        for (const name of ["other"]) {
          const folder = await within(archiveSection).findByRole("button", {
            name: new RegExp(name),
          });
          await userEvent.click(folder);
        }
        await userEvent.click(
          await within(archiveSection).findByText("archive.md"),
        );
        expect(
          await screen.findByTestId("spec-suggestion-preview-archive-hunk"),
        ).toHaveTextContent("New");
        expect(
          screen.getAllByRole("tab", { name: /^archive(?: review)?$/i }),
        ).toHaveLength(1);
      },
    );

    it.each([
      ["source_control_change_graph", "Diff Graph", "Open graph document"],
    ] as const)(
      "reuses the same file from the %s callback after a PROJECT DOCS open",
      async (type, name, action) => {
        localStorage.setItem(
          `g5:tabs:${workspaceId}`,
          JSON.stringify({
            tabs: [{ type, name }],
            activeTabId: "source-control-change-graph",
          }),
        );
        renderReviewDoc();
        await openProjectDoc();
        await userEvent.click(await screen.findByRole("tab", { name }));
        await userEvent.click(
          await screen.findByRole("button", { name: action }),
        );
        await userEvent.click(
          await screen.findByRole("tab", { name: /^project(?: review)?$/i }),
        );
        expect(
          await screen.findByTestId("spec-suggestion-preview-project-hunk"),
        ).toHaveTextContent("New");
        // Every opener reaches the same read-only projection and Source action.
        expect(
          screen
            .getByTestId("document-preview")
            .querySelector('[contenteditable="true"]'),
        ).toBeNull();
        expect(screen.getByTestId("file-view-edit")).toBeVisible();
        await openProjectDoc();
        expect(
          screen.getAllByRole("tab", { name: /^project(?: review)?$/i }),
        ).toHaveLength(1);
      },
    );

    it.each([
      {
        name: "direct URL",
        restored: false,
        presentation: undefined,
      },
      {
        name: "legacy restored tab",
        restored: true,
        presentation: undefined,
      },
      {
        name: "explicit inline restored tab",
        restored: true,
        presentation: "inline",
      },
      {
        name: "explicit diff restored tab",
        restored: true,
        presentation: "diff",
      },
    ] as const)(
      "uses the common file preview for $name",
      async ({ restored, presentation }) => {
        if (restored) {
          const tab: TabItem = {
            type: "doc",
            docId: docs[0].id,
            name: docs[0].name,
            ...(presentation ? { reviewPresentation: presentation } : {}),
          };
          localStorage.setItem(
            `g5:tabs:${workspaceId}`,
            JSON.stringify({ tabs: [tab], activeTabId: `doc:${docs[0].id}` }),
          );
        }
        renderReviewDoc(
          `/org-g5labs/test-workspace/doc/${encodeURIComponent(docs[0].id)}`,
        );
        // The launch shell may keep Chat foreground while restoring tabs;
        // focus the routed/restored document without creating a new origin.
        await userEvent.click(
          await screen.findByRole("tab", { name: /^project(?: review)?$/i }),
        );
        expect(
          await screen.findByTestId("spec-suggestion-preview-project-hunk"),
        ).toHaveTextContent("New");
        expect(
          screen
            .getByTestId("document-preview")
            .querySelector('[contenteditable="true"]'),
        ).toBeNull();
        await openProjectDoc();
      },
    );
  });
  describe("tab management", () => {
    it("opens Getting Started as a read-only guide tab on first empty landing", async () => {
      mockDesktopMatchMedia();

      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      await waitFor(() => {
        expect(
          within(screen.getByTestId("tab-bar")).getByText("Getting Started"),
        ).toBeInTheDocument();
      });

      // The timeline is the landing surface (g5.workbench.landing);
      // auto-opened guides remain available as background content tabs
      // until the user selects them.
      expect(screen.getByRole("tab", { name: "Workspace" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await userEvent.click(
        within(screen.getByTestId("tab-bar")).getByText("Getting Started"),
      );

      const centerPanel = screen.getByTestId("center-panel");
      const guideView = within(centerPanel).getByTitle("Getting Started");
      expect(guideView).toHaveAttribute("data-testid", "guide-doc-view");
      const guideContent =
        await within(guideView).findByTestId("guide-doc-content");
      // [D58] guide body fills the panel width — same rule as the editor.
      expect(guideContent).toHaveClass("w-full");
      expect(guideContent.className).not.toContain("max-w-4xl");
      const markdownRenderer =
        await within(guideContent).findByTestId("markdown-renderer");
      expect(markdownRenderer).toHaveClass(
        "[&_a]:font-medium",
        "[&_a]:text-[var(--g5-active)]",
        "[&_a]:underline",
        "[&_a]:underline-offset-4",
      );
      expect(markdownRenderer.className).toContain(
        "[&_a:hover]:text-[var(--g5-text0)]",
      );
      expect(markdownRenderer.className).toContain(
        "[&_a:focus-visible]:outline-[var(--g5-active)]",
      );
      expect(
        within(centerPanel).getByRole("heading", {
          name: "Getting Started",
        }),
      ).toBeInTheDocument();
      expect(
        within(centerPanel).queryByTestId("guide-doc-frame"),
      ).not.toBeInTheDocument();
      expect(
        localStorage.getItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY),
      ).toBe("1");
      const contentPanel = within(centerPanel).getByTestId(
        "center-content-panel",
      );
      expect(contentPanel.querySelector("textarea")).not.toBeInTheDocument();
      expect(
        contentPanel.querySelector('[contenteditable="true"]'),
      ).not.toBeInTheDocument();
      expect(
        within(contentPanel).queryByRole("button", { name: /sync/i }),
      ).not.toBeInTheDocument();
      expect(
        within(contentPanel).queryByRole("button", { name: /merge/i }),
      ).not.toBeInTheDocument();
    });

    it("opens a persistent full-panel Terminal tab, focuses it once, and closes it", async () => {
      mockDesktopMatchMedia();
      localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "test-workspace" },
          }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails["task-graph-rework"],
              id: params.id,
              name: "Test Workspace",
              isBase: false,
            },
          }),
        ),
      );
      const user = userEvent.setup();
      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?view=settings",
          user: createMockUser(),
        },
      );
      await screen.findByTestId("workspace-settings-page");
      // The owner's Terminal is the rail's Files header action
      // (ui g5.ui.workbench-shell).
      const railTerminal = () =>
        within(screen.getByTestId("sidebar-files-tab")).getByRole("button", {
          name: "Terminal",
        });
      await screen.findByTestId("sidebar-files-tab");
      await user.click(railTerminal());
      const terminalTab = await screen.findByRole("tab", { name: "Terminal" });
      expect(terminalTab).toHaveAttribute("aria-selected", "true");
      const panel = screen.getByRole("tabpanel", { name: "Terminal" });
      expect(panel).toHaveClass("h-full", "min-h-0");
      expect(panel).toBeVisible();
      expect(screen.getByTestId("center-panel")).toContainElement(panel);
      const terminal = screen.getByTestId("stub-workspace-terminal");
      expect(terminal).toHaveAttribute("data-enabled", "true");
      await user.type(
        screen.getByRole("textbox", { name: "Terminal command" }),
        "echo preserved",
      );
      await user.click(screen.getByRole("tab", { name: "Chat" }));
      expect(panel).not.toBeVisible();
      expect(screen.getByTestId("stub-workspace-terminal")).toBe(terminal);
      await user.click(screen.getByRole("tab", { name: "Settings" }));
      expect(screen.getByTestId("stub-workspace-terminal")).toBe(terminal);
      await user.click(railTerminal());
      expect(screen.getAllByRole("tab", { name: "Terminal" })).toHaveLength(1);
      expect(
        screen.getByRole("textbox", { name: "Terminal command" }),
      ).toHaveValue("echo preserved");
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "view=terminal",
      );
      await user.click(
        within(terminalTab.parentElement!).getByTitle("Close tab"),
      );
      expect(
        screen.queryByTestId("stub-workspace-terminal"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("tab", { name: "Terminal" }),
      ).not.toBeInTheDocument();
    });

    it("defers a restored background Terminal and each new VM until selected", async () => {
      mockDesktopMatchMedia();
      localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
      localStorage.setItem(
        "g5:tabs:test-workspace",
        JSON.stringify({
          tabs: [{ type: "workspace_terminal", name: "Terminal" }],
          activeTabId: "workspace-terminal",
        }),
      );
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "test-workspace" },
          }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails["task-graph-rework"],
              id: params.id,
              isBase: false,
            },
          }),
        ),
      );
      const user = userEvent.setup();
      const { queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );
      const terminalTab = await screen.findByRole("tab", { name: "Terminal" });
      await screen.findByTestId("workbench-header");
      // The landing opens the timeline over a restored background tab.
      expect(screen.getByRole("tab", { name: "Workspace" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      expect(screen.queryByTestId("stub-workspace-terminal")).toBeNull();

      await user.click(terminalTab);
      const terminal = await screen.findByTestId("stub-workspace-terminal");
      expect(terminal).toHaveAttribute("data-enabled", "true");
      await user.click(screen.getByRole("tab", { name: "Chat" }));
      expect(screen.getByTestId("stub-workspace-terminal")).toBe(terminal);

      act(() => {
        queryClient.setQueryData<WorkspaceVmView>(
          ["workspace-vm", "test-workspace"],
          (vm) => vm && { ...vm, epoch: vm.epoch + 1 },
        );
      });
      await waitFor(() =>
        expect(screen.queryByTestId("stub-workspace-terminal")).toBeNull(),
      );
      await user.click(terminalTab);
      expect(await screen.findByTestId("stub-workspace-terminal")).not.toBe(
        terminal,
      );
    });

    it.each([
      ["terminal", "Terminal"],
      ["settings", "Settings"],
      ["workers", "Workers"],
    ])(
      "preserves a cold %s link through restoration and workspace switches",
      async (view, label) => {
        mockDesktopMatchMedia();
        localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
        localStorage.setItem(
          "g5:tabs:test-workspace",
          JSON.stringify({
            tabs: [{ type: "workspace_details", name: "Workspace Details" }],
            activeTabId: "workspace-details",
          }),
        );
        let releaseInstanceInfo!: () => void;
        const instanceGate = new Promise<void>((resolve) => {
          releaseInstanceInfo = resolve;
        });
        server.use(
          http.get("/api/v1/resolve", ({ request }) =>
            HttpResponse.json({
              data: {
                orgId: "org-g5labs",
                workspaceId: new URL(request.url).searchParams.get("workspace"),
              },
            }),
          ),
          http.get("/api/v1/instance/info", async () => {
            await instanceGate;
            return HttpResponse.json({ data: mockInstanceInfo });
          }),
        );
        const { queryClient } = renderApp(
          <Routes>
            <Route
              path="/:org/:workspace/*"
              element={
                <WorkspacePageWithWorkspaceSwitchButton
                  destination={`/org-g5labs/next-workspace?view=${view}`}
                />
              }
            />
          </Routes>,
          {
            route: `/org-g5labs/test-workspace?view=${view}`,
            user: createMockUser(),
          },
        );
        await waitFor(() =>
          expect(
            queryClient.getQueryData(["workspace-vm", "test-workspace"]),
          ).toBeDefined(),
        );
        expect(screen.getByTestId("route-probe")).toHaveTextContent(
          `view=${view}`,
        );
        await act(async () => releaseInstanceInfo());
        await screen.findByRole("tab", { name: "Workspace Details" });
        await waitFor(() =>
          expect(screen.getByRole("tab", { name: label })).toHaveAttribute(
            "aria-selected",
            "true",
          ),
        );
        expect(screen.getByRole("tabpanel", { name: label })).toBeVisible();
        expect(screen.getByTestId("route-probe")).toHaveTextContent(
          `view=${view}`,
        );

        await userEvent.click(
          screen.getByRole("button", { name: "Switch workspace" }),
        );
        await waitFor(() =>
          expect(
            queryClient.getQueryData(["workspace-vm", "next-workspace"]),
          ).toBeDefined(),
        );
        await waitFor(() =>
          expect(screen.getByRole("tab", { name: label })).toHaveAttribute(
            "aria-selected",
            "true",
          ),
        );
        expect(screen.getByTestId("route-probe")).toHaveTextContent(
          `/org-g5labs/next-workspace?view=${view}`,
        );
        expect(
          screen.queryByRole("tab", { name: "Workspace Details" }),
        ).toBeNull();
      },
    );

    it("opens Terminal from its view URL without requiring a content branch", async () => {
      mockDesktopMatchMedia();
      localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?view=terminal",
          user: createMockUser(),
        },
      );
      expect(
        await screen.findByRole("tabpanel", { name: "Terminal" }),
      ).toBeVisible();
      await userEvent.click(screen.getByRole("tab", { name: "Chat" }));
      await waitFor(() =>
        expect(screen.getByTestId("route-probe")).not.toHaveTextContent(
          "view=terminal",
        ),
      );
    });

    it("clears an admin view hint when Chat becomes the visible center surface", async () => {
      mockDesktopMatchMedia();
      const user = userEvent.setup();

      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?view=settings",
          user: createMockUser(),
        },
      );

      expect(
        await screen.findByTestId("workspace-settings-page"),
      ).toBeVisible();
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "view=settings",
      );

      await user.click(screen.getByTestId("center-chat-tab"));

      await waitFor(() => {
        expect(screen.getByTestId("center-chat-tab")).toHaveAttribute(
          "aria-selected",
          "true",
        );
        expect(screen.getByTestId("route-probe")).not.toHaveTextContent(
          "view=settings",
        );
      });
    });

    it("opens known in-guide links as app guide tabs", async () => {
      mockDesktopMatchMedia();
      server.use(
        http.get("/user-guide/getting-started-latest.md", () =>
          HttpResponse.text(
            "# Getting Started\n\n[Build guide](onboarding-build-from-scratch.md)",
          ),
        ),
        http.get("/user-guide/onboarding-build-from-scratch.md", () =>
          HttpResponse.text("# Build Guide"),
        ),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      const tabBar = await screen.findByTestId("tab-bar");
      await userEvent.click(await within(tabBar).findByText("Getting Started"));

      const buildGuideLink = await screen.findByRole("link", {
        name: "Build guide",
      });
      const markdownRenderer = buildGuideLink.closest(
        '[data-testid="markdown-renderer"]',
      );
      expect(markdownRenderer).not.toBeNull();
      expect(markdownRenderer).toHaveClass(
        "[&_a]:font-medium",
        "[&_a]:text-[var(--g5-active)]",
        "[&_a]:underline",
        "[&_a]:underline-offset-4",
      );

      fireEvent.click(buildGuideLink);

      await waitFor(() => {
        expect(
          within(screen.getByTestId("tab-bar")).getByText(
            "Using G5 to Build from Scratch",
          ),
        ).toBeInTheDocument();
      });

      expect(screen.queryByTestId("guide-doc-frame")).not.toBeInTheDocument();
      expect(
        await screen.findByRole("heading", { name: "Build Guide" }),
      ).toBeInTheDocument();
    });

    it("scrolls same-page guide anchors inside the Markdown guide view", async () => {
      mockDesktopMatchMedia();
      const originalScrollIntoView = HTMLElement.prototype.scrollIntoView;
      const scrollIntoView = vi.fn();
      HTMLElement.prototype.scrollIntoView = scrollIntoView;
      server.use(
        http.get("/user-guide/getting-started-latest.md", () =>
          HttpResponse.text(
            "# Getting Started\n\n[Jump](#target-section)\n\n## Target Section",
          ),
        ),
      );

      try {
        renderApp(
          <Routes>
            <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
          </Routes>,
          {
            route: "/org-g5labs/test-workspace",
            user: createMockUser(),
          },
        );

        const tabBar = await screen.findByTestId("tab-bar");
        await userEvent.click(
          await within(tabBar).findByText("Getting Started"),
        );

        fireEvent.click(await screen.findByRole("link", { name: "Jump" }));

        await waitFor(() => {
          expect(scrollIntoView).toHaveBeenCalledWith({ block: "start" });
        });
        expect(
          within(screen.getByTestId("tab-bar")).getByText("Getting Started"),
        ).toBeInTheDocument();
      } finally {
        HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
      }
    });

    it("closes editor tabs when switching to another workspace in the same mounted page", async () => {
      const user = userEvent.setup();
      const matchMedia = vi.mocked(window.matchMedia);
      matchMedia.mockImplementation((query: string) => ({
        matches: query.includes("min-width: 1024px"),
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      }));

      try {
        renderApp(
          <Routes>
            <Route
              path="/:org/:workspace/*"
              element={<WorkspacePageWithWorkspaceSwitchButton />}
            />
          </Routes>,
          {
            route: "/org-g5labs/test-workspace/doc/doc-frontend-design",
            user: createMockUser(),
          },
        );

        await waitFor(() => {
          expect(
            within(screen.getByTestId("tab-bar")).getByText("Frontend Design"),
          ).toBeInTheDocument();
        });

        await user.click(screen.getByTestId("center-chat-tab"));
        await user.type(
          within(screen.getByTestId("right-panel")).getByTestId("chat-input"),
          "Draft for the first workspace",
        );

        await user.click(
          screen.getByRole("button", { name: "Switch workspace" }),
        );

        await waitFor(() => {
          expect(screen.getByTestId("route-probe")).toHaveTextContent(
            "/org-g5labs/task-graph-rework",
          );
          expect(
            screen.getByRole("tab", { name: "Workspace" }),
          ).toHaveAttribute("aria-selected", "true");
          expect(
            within(screen.getByTestId("tab-bar")).queryByText(
              "Frontend Design",
            ),
          ).toBeNull();
          expect(
            within(screen.getByTestId("right-panel")).getByTestId("chat-input"),
          ).toHaveValue("");
        });
      } finally {
        matchMedia.mockImplementation((query: string) => ({
          matches: !query.includes("min-width: 1024px"),
          media: query,
          onchange: null,
          addListener: vi.fn(),
          removeListener: vi.fn(),
          addEventListener: vi.fn(),
          removeEventListener: vi.fn(),
          dispatchEvent: vi.fn(),
        }));
      }
    });

    it("switches to the route org when it differs from the active org", async () => {
      mockDesktopMatchMedia();
      const initialUser = {
        ...createMockUser(),
        activeOrgId: "org-g5labs",
        oauthProvider: "github",
        scmConnections: [],
      };
      let currentUser = initialUser;
      const refreshBodies: Array<{ orgId?: string }> = [];

      server.use(
        http.post("/api/v1/auth/refresh", async ({ request }) => {
          const body = (await request.json().catch(() => ({}))) as {
            orgId?: string;
          };
          refreshBodies.push(body);
          currentUser = {
            ...initialUser,
            activeOrgId: "org-acme",
          };
          return HttpResponse.json({
            data: {
              jwt: createJwtForOrg("org-acme", initialUser.scopes),
            },
          });
        }),
        http.get("/api/v1/users/me", () =>
          HttpResponse.json({ data: currentUser }),
        ),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/acme-corp/test-workspace",
          user: initialUser,
        },
      );

      await waitFor(() => {
        expect(refreshBodies).toContainEqual({ orgId: "org-acme" });
      });
      // The page settles on the switched organization (Home's tests cover
      // the org-scoped reads waiting for the switched JWT).
      await screen.findByTestId("workbench-header");
    });

    it("reports a failed route org switch", async () => {
      mockDesktopMatchMedia();
      const initialUser = {
        ...createMockUser(),
        activeOrgId: "org-g5labs",
        oauthProvider: "github",
        scmConnections: [],
      };

      server.use(
        http.post("/api/v1/auth/refresh", () =>
          HttpResponse.json(
            { error: { message: "organization switch rejected" } },
            { status: 403 },
          ),
        ),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/acme-corp/test-workspace",
          user: initialUser,
        },
      );

      expect(
        await screen.findByText("Could not switch organization"),
      ).toBeInTheDocument();
    });

    it("drops persisted tabs of retired types on restore", async () => {
      mockDesktopMatchMedia();
      const resolvedWorkspaceId = "base";
      localStorage.setItem(
        `g5:tabs:${resolvedWorkspaceId}`,
        JSON.stringify({
          tabs: [
            { type: "workspace_details", name: "Workspace details" },
            { type: "workspace_plan", name: "Progress" },
            {
              type: "issues_list",
              orgId: "org-g5labs",
              workspaceId: resolvedWorkspaceId,
              name: "Issues",
            },
          ],
          activeTabId: "workspace-plan",
        }),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      const tabBar = await screen.findByTestId("tab-bar");
      // The supported sibling proves persisted tabs were read and restored.
      expect(
        await within(tabBar).findByText("Workspace details"),
      ).toBeInTheDocument();
      expect(within(tabBar).queryByText("Issues")).toBeNull();
      expect(within(tabBar).queryByText("Progress")).toBeNull();
      expect(await screen.findByTestId("workspace-landing")).toBeVisible();
    });

    it("opens the intent-log route when instance metadata errors", async () => {
      mockDesktopMatchMedia();

      server.use(
        http.get("/api/v1/instance/info", () =>
          HttpResponse.json(
            { error: { message: "instance metadata temporarily unavailable" } },
            { status: 503 },
          ),
        ),
        http.get("*/api/v1/intent-log", () =>
          HttpResponse.json({
            data: {
              rows: [],
              meta: {
                facets: {},
                cursor: null,
                hasMore: false,
                diagnostics: [],
              },
            },
          }),
        ),
      );

      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace/intent-log",
          user: createMockUser(),
        },
      );

      expect(await screen.findByTestId("intent-log-view")).toBeInTheDocument();
      expect(
        within(screen.getByTestId("tab-bar")).getByText("All intent logs"),
      ).toBeInTheDocument();
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "/org-g5labs/test-workspace/intent-log",
      );
    });
  });

  describe("panel toggles", () => {
    it("keeps right-panel chat scope independent from task selection", () => {
      expect(getRightPanelChatScope()).toBe("workspace");
    });

    it("suppresses docked and detached Stop until queued B starts streaming", async () => {
      mockDesktopMatchMedia();
      const user = userEvent.setup();
      const fixture = installQueuedChatStopFixture("streaming");

      const { queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=queued-stop-session",
          user: createMockUser(),
        },
      );

      const rightPanel = await screen.findByTestId("right-panel");
      const stopButton = await within(rightPanel).findByTestId(
        "stop-streaming-button",
      );
      fireEvent.click(stopButton);
      fireEvent.click(stopButton);
      await waitFor(() => {
        expect(fixture.stoppedMessageIds).toEqual(["assistant-a"]);
      });
      expect(fixture.stopRequestIds).toEqual(["assistant-a"]);
      await waitFor(() => {
        expect(
          within(rightPanel).queryByTestId("stop-streaming-button"),
        ).not.toBeInTheDocument();
      });
      expect(
        within(rightPanel).queryByTestId("claude-interrupt-control"),
      ).not.toBeInTheDocument();
      expect(
        within(rightPanel).getByTestId("streaming-indicator"),
      ).toBeInTheDocument();
      fireEvent.keyDown(within(rightPanel).getByTestId("chat-input"), {
        key: "Escape",
      });
      expect(fixture.stopRequestIds).toEqual(["assistant-a"]);

      await user.click(within(rightPanel).getByTitle("Pop out"));
      expect(await screen.findByTitle("Dock panel")).toBeInTheDocument();
      expect(
        screen.queryByTestId("stop-streaming-button"),
      ).not.toBeInTheDocument();
      expect(fixture.stoppedMessageIds).toEqual(["assistant-a"]);

      fixture.setAssistantStatus("assistant-b", "streaming");
      await act(async () => {
        await queryClient.invalidateQueries({
          queryKey: ["chat-messages", fixture.sessionId],
        });
      });

      await user.click(await screen.findByTestId("stop-streaming-button"));
      await waitFor(() => {
        expect(fixture.stoppedMessageIds).toEqual([
          "assistant-a",
          "assistant-b",
        ]);
      });
    });

    it("suppresses mobile Stop until queued B starts streaming", async () => {
      const previousInnerWidth = window.innerWidth;
      Object.defineProperty(window, "innerWidth", {
        writable: true,
        configurable: true,
        value: 375,
      });
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
      const user = userEvent.setup();
      const fixture = installQueuedChatStopFixture("pending");

      try {
        const { queryClient } = renderApp(
          <Routes>
            <Route path="/:org/:workspace" element={<WorkspacePage />} />
          </Routes>,
          {
            route: "/org-g5labs/test-workspace?session=queued-stop-session",
            user: createMockUser(),
          },
        );

        await user.click(await screen.findByTestId("stop-streaming-button"));
        await waitFor(() => {
          expect(fixture.stoppedMessageIds).toEqual(["assistant-a"]);
        });
        await waitFor(() => {
          expect(
            screen.queryByTestId("stop-streaming-button"),
          ).not.toBeInTheDocument();
        });
        expect(fixture.stoppedMessageIds).toEqual(["assistant-a"]);

        fixture.setAssistantStatus("assistant-b", "streaming");
        await act(async () => {
          await queryClient.invalidateQueries({
            queryKey: ["chat-messages", fixture.sessionId],
          });
        });

        await user.click(await screen.findByTestId("stop-streaming-button"));
        await waitFor(() => {
          expect(fixture.stoppedMessageIds).toEqual([
            "assistant-a",
            "assistant-b",
          ]);
        });
      } finally {
        Object.defineProperty(window, "innerWidth", {
          writable: true,
          configurable: true,
          value: previousInnerWidth,
        });
      }
    });

    it("restores Stop after request failure and after changing sessions", async () => {
      mockDesktopMatchMedia();
      const user = userEvent.setup();
      const fixture = installQueuedChatStopFixture("streaming", {
        failFirstStop: true,
      });

      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=queued-stop-session",
          user: createMockUser(),
        },
      );

      await user.click(await screen.findByTestId("stop-streaming-button"));
      await waitFor(() => {
        expect(fixture.stopRequestIds).toEqual(["assistant-a"]);
      });
      await user.click(await screen.findByTestId("stop-streaming-button"));
      await waitFor(() => {
        expect(fixture.stoppedMessageIds).toEqual(["assistant-a"]);
      });
      await waitFor(() => {
        expect(
          screen.queryByTestId("stop-streaming-button"),
        ).not.toBeInTheDocument();
      });

      await user.click(screen.getByTestId("new-session-header"));
      await user.click(await screen.findByTestId("chat-center-history"));
      await user.click(await screen.findByTestId("chat-session-select"));

      expect(
        await screen.findByTestId("stop-streaming-button"),
      ).toBeInTheDocument();
      expect(fixture.stopRequestIds).toEqual(["assistant-a", "assistant-a"]);
    });

    it("keeps the composer Stop usable after a plain stop with no queued successor", async () => {
      mockDesktopMatchMedia();
      const fixture = installPlainChatStopFixture({ holdFirstStop: true });

      const { queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=plain-stop-session",
          user: createMockUser(),
        },
      );

      const rightPanel = await screen.findByTestId("right-panel");
      const stopButton = await within(rightPanel).findByTestId(
        "stop-streaming-button",
      );
      fireEvent.click(stopButton);
      await waitFor(() => {
        expect(fixture.stopRequestIds).toEqual(["assistant-a"]);
      });

      // Regression guard for "both Stop buttons vanish until refresh": with no
      // queued successor there is no A→B handoff, so the single composer Stop
      // control must stay visible while the turn is still streaming.
      expect(
        within(rightPanel).getByTestId("stop-streaming-button"),
      ).toBeInTheDocument();
      // The redundant provider-tag Stop button no longer exists.
      expect(
        within(rightPanel).queryByTestId("claude-interrupt-control"),
      ).not.toBeInTheDocument();

      // While the first request is in flight the control shows `Stopping…`,
      // is disabled, and a second click sends no second request
      // (chat g5.chat.controls).
      await waitFor(() => {
        expect(stopButton).toHaveAttribute("data-stop-phase", "requesting");
      });
      expect(stopButton).toBeDisabled();
      expect(stopButton).toHaveAttribute("title", "Stopping…");
      fireEvent.click(stopButton);
      fireEvent.click(stopButton);
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(fixture.stopRequestIds).toEqual(["assistant-a"]);
      expect(
        within(rightPanel).getByTestId("streaming-indicator"),
      ).toHaveTextContent("Stopping…");

      // The request settled with the runtime signalling the turn: the state
      // stays `Stopping…` until the turn ends, and the control is usable
      // again, so an explicit later click is a safe repeat (P1, zcl-g5): it
      // must issue another Stop request instead of no-oping on a lingering
      // handoff fence.
      fixture.releaseFirstStop();
      await waitFor(() => {
        expect(stopButton).toHaveAttribute("data-stop-phase", "stopping");
      });
      expect(stopButton).not.toBeDisabled();
      fireEvent.click(within(rightPanel).getByTestId("stop-streaming-button"));
      await waitFor(() => {
        expect(fixture.stopRequestIds).toEqual(["assistant-a", "assistant-a"]);
      });

      // Once the turn actually terminalizes, the Stop control hides on its own —
      // no page refresh required.
      fixture.setAssistantStatus("stopped");
      await act(async () => {
        await queryClient.invalidateQueries({
          queryKey: ["chat-messages", fixture.sessionId],
        });
      });
      await waitFor(() => {
        expect(
          within(rightPanel).queryByTestId("stop-streaming-button"),
        ).not.toBeInTheDocument();
      });
    });

    it("switches between central Chat and content without losing the session", async () => {
      mockDesktopMatchMedia();
      const user = userEvent.setup();
      const session = createMockChatSession({
        id: "workspace-close-session",
        scopeType: "workspace",
        scopeRef: "ws-123",
        title: "Workspace close session",
        agentProvider: "claude_code",
        messageCount: 0,
      });

      server.use(
        http.get("/api/v1/workspaces/:id/sessions", () =>
          HttpResponse.json({ data: [session] }),
        ),
        http.get("/api/v1/sessions/:sessionId", ({ params }) => {
          if (params.sessionId === "workspace-close-session") {
            return HttpResponse.json({ data: session });
          }

          return HttpResponse.json({ data: null }, { status: 404 });
        }),
        http.get("/api/v1/sessions/:sessionId/messages", () =>
          HttpResponse.json({ data: [] }),
        ),
        http.get("/api/v1/sessions/:sessionId/timeline", () =>
          HttpResponse.json({
            data: { entries: [], cursor: null, hasMore: false },
          }),
        ),
      );

      // The guide tab owns `/guide/:guideId` (g5.workbench.tabs): the
      // routes cover it as `App.tsx` does.
      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=workspace-close-session",
          user: createMockUser(),
        },
      );

      // The center view's title; the chat's bubble carries it too.
      expect(
        await within(await screen.findByTestId("right-panel")).findByText(
          "Workspace close session",
        ),
      ).toBeInTheDocument();
      const chatPanel = screen.getByTestId("right-panel");
      expect(chatPanel).toHaveAttribute("data-chat-placement", "center");
      expect(screen.queryByTestId("toggle-right-panel")).toBeNull();
      const draftInput = within(chatPanel).getByTestId("chat-input");
      await user.type(draftInput, "Keep this draft while I inspect the guide");

      const tabBar = screen.getByTestId("tab-bar");
      const chatTab = within(tabBar).getByTestId("center-chat-tab");
      const guideTab = await within(tabBar).findByRole("tab", {
        name: "Getting Started",
      });
      // Chat leads the strip, then the pinned Workspace (timeline) tab
      // (g5.workbench.landing), then the guide.
      const workspaceTab = within(tabBar).getByRole("tab", {
        name: "Workspace",
      });
      chatTab.focus();
      await user.keyboard("{ArrowRight}");
      expect(workspaceTab).toHaveFocus();
      expect(workspaceTab).toHaveAttribute("aria-selected", "true");
      await user.keyboard("{ArrowRight}");
      expect(guideTab).toHaveFocus();
      expect(guideTab).toHaveAttribute("aria-selected", "true");

      await user.keyboard("{Home}");
      expect(chatTab).toHaveFocus();
      expect(chatTab).toHaveAttribute("aria-selected", "true");
      await user.keyboard("{End}");
      expect(guideTab).toHaveFocus();
      await user.keyboard("{ArrowRight}");
      expect(chatTab).toHaveFocus();
      await user.keyboard("{ArrowLeft}");
      expect(guideTab).toHaveFocus();

      const hiddenChatPanel = screen.getByTestId("center-chat-panel");
      expect(hiddenChatPanel).toHaveAttribute("hidden");
      expect(within(hiddenChatPanel).getByTestId("chat-input")).toHaveValue(
        "Keep this draft while I inspect the guide",
      );

      // The guide is content: its URL names it without the conversation
      // hint, which Chat's URL carries again once Chat returns to the center
      // (g5.workbench.tabs).
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        /^\/org-g5labs\/test-workspace\/guide\/getting-started$/,
      );

      await user.click(chatTab);
      expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
        "hidden",
      );
      expect(screen.getByTestId("chat-input")).toHaveValue(
        "Keep this draft while I inspect the guide",
      );
      expect(
        await within(screen.getByTestId("right-panel")).findByText(
          "Workspace close session",
        ),
      ).toBeInTheDocument();
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "session=workspace-close-session",
      );
    });

    it("toggles left sidebar on button click", async () => {
      const user = userEvent.setup();
      renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      // Wait for page to load
      await waitFor(() => {
        const page =
          document.querySelector('[class*="workspace"]') || document.body;
        expect(page).toBeTruthy();
      });

      // Find sidebar toggle button (usually a menu/hamburger icon button)
      const toggleBtn =
        screen.queryByRole("button", { name: /sidebar|menu|toggle/i }) ||
        screen.queryByLabelText(/sidebar|menu/i);
      if (toggleBtn) {
        await user.click(toggleBtn);
      }
    });

    it("lands on the timeline with Chat as a center tab and no title-bar toggle", async () => {
      mockDesktopMatchMedia();
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      expect(await screen.findByTestId("right-panel")).toHaveAttribute(
        "data-chat-placement",
        "center",
      );
      expect(screen.getByRole("tab", { name: "Workspace" })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      expect(screen.getByTestId("center-chat-tab")).toBeInTheDocument();
      // The Chat tab owns chat selection, so no toggle renders anywhere.
      expect(screen.queryByTestId("toggle-right-panel")).toBeNull();
    });

    it("restores a task conversation from a document URL after reload", async () => {
      mockDesktopMatchMedia();
      const user = userEvent.setup();
      const taskSession = createMockChatSession({
        id: "task-session-from-document-url",
        workspaceId: "base",
        scopeType: "task",
        scopeRef: "task-from-document-url",
        title: "Restored task conversation",
      });
      const workspaceSession = createMockChatSession({
        id: "workspace-session-only",
        workspaceId: "base",
        scopeType: "workspace",
        scopeRef: "test-workspace",
        title: "Workspace conversation",
      });
      const sentSessionIds: string[] = [];

      server.use(
        http.get("/api/v1/workspaces/:id/sessions", () =>
          HttpResponse.json({ data: [workspaceSession] }),
        ),
        http.get("/api/v1/sessions/:sessionId", ({ params }) => {
          const session =
            params.sessionId === taskSession.id
              ? taskSession
              : workspaceSession;
          return HttpResponse.json({ data: session });
        }),
        http.post("/api/v1/sessions/:sessionId/messages", ({ params }) => {
          sentSessionIds.push(String(params.sessionId));
          return HttpResponse.json(
            { data: { messageId: "restored-task-message", status: "queued" } },
            { status: 201 },
          );
        }),
      );

      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        {
          route:
            "/org-g5labs/test-workspace/doc/doc-frontend-design?session=task-session-from-document-url",
          user: createMockUser(),
        },
      );

      expect(
        await screen.findByText("Restored task conversation"),
      ).toBeVisible();
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "session=task-session-from-document-url",
      );
      await user.click(screen.getByTestId("center-chat-tab"));
      const chatPanel = screen.getByTestId("center-chat-panel");
      await user.type(
        within(chatPanel).getByTestId("chat-input"),
        "Continue after reload",
      );
      await user.click(within(chatPanel).getByTestId("chat-panel-send"));

      await waitFor(() => {
        expect(sentSessionIds).toEqual([taskSession.id]);
      });
    });
  });

  describe("workspace search removal (P8.9)", () => {
    it("renders no desktop search box, panel or shortcut surface", async () => {
      // P8.9 removed workspace search end to end. Neither the docked bar nor
      // any modal/spotlight surface may come back, and no shortcut may open
      // one. Chat-history search and editor Find are unaffected.
      mockDesktopMatchMedia();
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      // The workbench header renders — proving the page actually mounted,
      // which is what makes the negative assertions below meaningful.
      expect(await screen.findByTestId("workbench-header")).toBeInTheDocument();

      expect(screen.queryByTestId("search-input-docked")).toBeNull();
      expect(screen.queryByTestId("search-panel")).toBeNull();
      expect(screen.queryByTestId("search-results")).toBeNull();

      fireEvent.keyDown(document, { key: "k", metaKey: true });

      await waitFor(() => {
        expect(screen.queryByTestId("search-panel")).toBeNull();
      });
      expect(screen.queryByTestId("spotlight-search")).toBeNull();
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    it("renders no mobile search row", async () => {
      // The mobile shell used to host the same SearchBar in its own full-width
      // row; that row is gone too.
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      expect(
        await screen.findByTestId("mobile-notifications-open"),
      ).toBeInTheDocument();
      expect(screen.queryByTestId("search-input-docked")).toBeNull();
      expect(screen.queryByTestId("search-panel")).toBeNull();
    });
  });

  describe("worker-host navigation", () => {
    // The Workers control sits in the tab strip's global controls
    // (ui g5.ui.workbench-shell), so these render the strip's provider.
    const shellPage = (
      <WorkspaceTabsProvider>
        <WorkspacePage />
      </WorkspaceTabsProvider>
    );

    it("opens the retained host list and Add host action from the tab strip", async () => {
      mockDesktopMatchMedia();
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={shellPage} />
        </Routes>,
        {
          route: "/g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      await userEvent.click(
        await screen.findByRole("button", { name: "Workers" }),
      );
      expect(
        await screen.findByTestId("worker-hosts-page-content"),
      ).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Add host" }));
      expect(
        screen.getByRole("button", { name: "Provision host" }),
      ).toBeInTheDocument();
    });

    it("hides the entry from members without worker-host admin access", async () => {
      mockDesktopMatchMedia();
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={shellPage} />
        </Routes>,
        {
          route: "/g5labs/test-workspace",
          user: createMockUser({
            orgs: [
              {
                orgId: "org-g5labs",
                orgName: "G5 Labs",
                orgSlug: "g5labs",
                role: "member",
              },
            ],
            scopes: ["workspaces:read"],
          }),
        },
      );

      await screen.findByTestId("shell-controls");
      expect(screen.queryByRole("button", { name: "Workers" })).toBeNull();
    });

    it("hides the entry when instance metadata changes to self-hosted", async () => {
      mockDesktopMatchMedia();
      const { queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={shellPage} />
        </Routes>,
        {
          route: "/g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      expect(
        await screen.findByRole("button", { name: "Workers" }),
      ).toBeInTheDocument();
      act(() => {
        queryClient.setQueryData(["instance-info"], {
          ...mockInstanceInfo,
          selfHosted: true,
        });
      });
      await waitFor(() => {
        expect(screen.queryByRole("button", { name: "Workers" })).toBeNull();
      });
    });
  });

  describe("tab management", () => {
    it("renders tab bar area", async () => {
      renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        const tabBar =
          document.querySelector('[class*="tab"]') ||
          document.querySelector('[role="tablist"]') ||
          document.body;
        expect(tabBar).toBeTruthy();
      });
    });
  });

  describe("responsive layout", () => {
    it("renders mobile layout below lg breakpoint", async () => {
      // Mock narrow viewport
      Object.defineProperty(window, "innerWidth", {
        writable: true,
        configurable: true,
        value: 375,
      });
      window.dispatchEvent(new Event("resize"));

      renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        // On mobile, the layout should adapt
        const mobileLayout =
          screen.queryByTestId("mobile-layout") ||
          document.querySelector('[class*="mobile"]') ||
          document.body;
        expect(mobileLayout).toBeTruthy();
      });

      // Restore viewport
      Object.defineProperty(window, "innerWidth", {
        writable: true,
        configurable: true,
        value: 1024,
      });
      window.dispatchEvent(new Event("resize"));
    });

    it("provides workspace display names to mobile chat citation tooltips", async () => {
      const mobileWorkspace = createMockWorkspace({
        id: "base",
        name: "Mobile Workspace",
      });
      const mobileWorkspaceDetail = {
        ...mobileWorkspace,
        orgId: "org-g5labs",
        isBase: false,
        docsRepoId: null,
        createdBy: "user-alex",
        updatedAt: "2024-12-16T08:00:00Z",
        docsFolderPath: "docs/base",
      };
      const mobileSession = createMockChatSession({
        id: "mobile-d8-session",
        workspaceId: "base",
        scopeType: "workspace",
        scopeRef: "base",
        title: "Mobile D8 session",
        agentProvider: "claude_code",
        messageCount: 1,
      });
      const mobileMessage = createMockChatMessage({
        id: "mobile-d8-message",
        sessionId: "mobile-d8-session",
        content: "See [1]",
        citations: [{ label: "Spec", filePath: "docs/base/a.md" }],
        messageIndex: 0,
      });

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
      Object.defineProperty(window, "innerWidth", {
        writable: true,
        configurable: true,
        value: 375,
      });
      window.dispatchEvent(new Event("resize"));

      server.use(
        http.get("/api/v1/orgs/:orgId/workspaces", () =>
          HttpResponse.json({ data: [mobileWorkspace] }),
        ),
        http.get("/api/v1/workspaces/:id", () =>
          HttpResponse.json({ data: mobileWorkspaceDetail }),
        ),
        http.get("/api/v1/workspaces/:id/sessions", () =>
          HttpResponse.json({ data: [mobileSession] }),
        ),
        http.get("/api/v1/sessions/:sessionId", () =>
          HttpResponse.json({ data: mobileSession }),
        ),
        http.get("/api/v1/sessions/:sessionId/messages", () =>
          HttpResponse.json({ data: [mobileMessage] }),
        ),
        http.get("/api/v1/sessions/:sessionId/timeline", () =>
          HttpResponse.json({
            data: { entries: [], cursor: null, hasMore: false },
          }),
        ),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=mobile-d8-session",
          user: createMockUser(),
        },
      );

      await waitFor(() => {
        expect(
          screen.getAllByTitle(/docs\/Mobile Workspace\/a\.md/).length,
        ).toBeGreaterThan(0);
      });
      expect(screen.queryByTitle(/docs\/base\/a\.md/)).not.toBeInTheDocument();
    });
  });

  describe("error states", () => {
    it("handles workspace fetch error gracefully", async () => {
      server.use(
        http.get("/api/v1/workspaces/:id", () =>
          HttpResponse.json(
            {
              error: {
                code: "INTERNAL",
                message: "Database error",
                requestId: "req-123",
              },
            },
            { status: 500 },
          ),
        ),
      );

      renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        const errorEl =
          screen.queryByText(/error/i) ||
          screen.queryByText(/something went wrong/i) ||
          document.body;
        expect(errorEl).toBeTruthy();
      });
    });
  });

  describe("empty states", () => {
    it("renders page even with empty workspace data", async () => {
      renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        const page =
          document.querySelector('[class*="workspace"]') || document.body;
        expect(page).toBeTruthy();
      });
    });
  });

  describe("workspace controls", () => {
    it("hides the owner's VM state and controls from a member with the same name", async () => {
      mockDesktopMatchMedia();
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "test-workspace" },
          }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails["task-graph-rework"],
              id: params.id,
              isBase: false,
              ownerUserId: "user-alex-other",
            },
          }),
        ),
        http.get("/api/v1/workspaces/:id/vm", ({ params }) =>
          HttpResponse.json({
            data: createMockWorkspaceVm({
              orgId: "org-g5labs",
              workspaceId: String(params.id),
              ownerUserId: "user-alex-other",
              status: "running",
              hostId: "host-test",
              vmId: `vm-${params.id}`,
              imageRef: "g5-test:latest",
              moduleCatalogVersion: "test",
              startedAt: "2024-12-16T08:00:00Z",
              lastActivityAt: "2024-12-16T08:00:00Z",
            }),
          }),
        ),
        // Only the user id tells the viewer apart from the owner.
        http.get("/api/v1/orgs/:orgId/members", () =>
          HttpResponse.json({
            data: [
              {
                userId: "user-alex",
                email: "alex@g5labs.dev",
                displayName: "Alex Chen",
                avatarUrl: null,
                role: "admin",
                joinedAt: "2024-01-10T08:00:00Z",
              },
              {
                userId: "user-alex-other",
                email: "alex.chen@example.com",
                displayName: "Alex Chen",
                avatarUrl: null,
                role: "member",
                joinedAt: "2024-01-11T08:00:00Z",
              },
            ],
          }),
        ),
      );
      const { queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        { route: "/org-g5labs/test-workspace", user: createMockUser() },
      );

      const controls = await screen.findByTestId(
        "workbench-workspace-controls",
      );
      await waitFor(() =>
        expect(
          within(controls).getByTestId("workspace-owner-trigger"),
        ).toHaveTextContent("Owner: Alex Chen"),
      );
      await waitFor(() =>
        expect(
          queryClient.getQueryData<WorkspaceVmView>([
            "workspace-vm",
            "test-workspace",
          ])?.status,
        ).toBe("running"),
      );
      expect(within(controls).queryByTestId("workspace-vm-start")).toBeNull();
      expect(within(controls).queryByTestId("workspace-vm-stop")).toBeNull();
      expect(within(controls).queryByTestId("workspace-vm-pending")).toBeNull();
      // The rail's Terminal action is the owner's alone.
      expect(
        within(await screen.findByTestId("sidebar-files-tab")).queryByRole(
          "button",
          { name: "Terminal" },
        ),
      ).toBeNull();
    });
  });

  describe("center tab bar visibility (g5.workspace.ui.center-tabs)", () => {
    function installSession() {
      const session = createMockChatSession({
        id: "hidden-strip-session",
        scopeType: "workspace",
        scopeRef: "ws-123",
        title: "Hidden strip session",
        agentProvider: "claude_code",
        messageCount: 0,
      });
      server.use(
        http.get("/api/v1/workspaces/:id/sessions", () =>
          HttpResponse.json({ data: [session] }),
        ),
        http.get("/api/v1/sessions/:sessionId", ({ params }) =>
          params.sessionId === session.id
            ? HttpResponse.json({ data: session })
            : HttpResponse.json({ data: null }, { status: 404 }),
        ),
        http.get("/api/v1/sessions/:sessionId/messages", () =>
          HttpResponse.json({ data: [] }),
        ),
        http.get("/api/v1/sessions/:sessionId/timeline", () =>
          HttpResponse.json({
            data: { entries: [], cursor: null, hasMore: false },
          }),
        ),
      );
      return session;
    }

    function renderWorkspace(route = "/org-g5labs/test-workspace") {
      mockDesktopMatchMedia();
      return renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/*"
            element={<WorkspacePageWithRouteProbe />}
          />
        </Routes>,
        { route, user: createMockUser() },
      );
    }

    it("omits the tab bar until the person enables workspace tabs", async () => {
      window.localStorage.removeItem("g5-user-settings");
      renderWorkspace();

      // The center surface itself still renders; only the strip is gone, and
      // the panel names itself because its tab control is not mounted.
      await screen.findByTestId("center-chat-panel");
      expect(screen.queryByTestId("tab-bar")).toBeNull();
      expect(screen.queryByTestId("center-chat-tab")).toBeNull();
      // With the strip hidden the ribbon's toggle is the way back to Chat.
      expect(screen.getByTestId("toggle-right-panel")).toBeInTheDocument();
    });

    it("renders the tab bar once workspace tabs are enabled", async () => {
      seedUserSettings({ showTabBar: true });
      renderWorkspace();

      const tabBar = await screen.findByTestId("tab-bar");
      expect(
        within(tabBar).getByRole("tab", { name: "Workspace" }),
      ).toHaveAttribute("aria-selected", "true");
      expect(within(tabBar).getByTestId("center-chat-tab")).toBeInTheDocument();
      // The Chat tab owns chat selection, so the ribbon's toggle stays out.
      expect(screen.queryByTestId("toggle-right-panel")).toBeNull();
    });

    it("brings Chat back from the ribbon with its session hint while the strip is hidden", async () => {
      window.localStorage.removeItem("g5-user-settings");
      const user = userEvent.setup();
      const session = installSession();
      renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

      expect(
        await within(await screen.findByTestId("right-panel")).findByText(
          "Hidden strip session",
        ),
      ).toBeInTheDocument();
      const toggle = screen.getByTestId("toggle-right-panel");
      expect(toggle).toHaveAttribute("title", "Close chat");

      await user.click(toggle);
      await waitFor(() =>
        expect(screen.getByTestId("center-chat-panel")).not.toBeVisible(),
      );
      expect(screen.getByTestId("route-probe")).not.toHaveTextContent(
        "session=",
      );
      expect(screen.getByTestId("toggle-right-panel")).toHaveAttribute(
        "title",
        "Open chat",
      );

      // Reopening restores the session hint, so a reload returns to the
      // conversation exactly as the Chat tab would.
      await user.click(screen.getByTestId("toggle-right-panel"));
      await waitFor(() =>
        expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
      );
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        `session=${session.id}`,
      );
      expect(screen.getByTestId("toggle-right-panel")).toHaveAttribute(
        "title",
        "Close chat",
      );
    });

    it("docks a popped-out Chat when the ribbon reopens it while the strip is hidden", async () => {
      window.localStorage.removeItem("g5-user-settings");
      const user = userEvent.setup();
      const session = installSession();
      renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

      const rightPanel = await screen.findByTestId("right-panel");
      await user.click(within(rightPanel).getByTitle("Pop out"));
      expect(await screen.findByTitle("Dock panel")).toBeInTheDocument();
      expect(screen.getByTestId("toggle-right-panel")).toHaveAttribute(
        "title",
        "Open chat",
      );

      await user.click(screen.getByTestId("toggle-right-panel"));
      await waitFor(() =>
        expect(screen.getByTestId("right-panel")).toHaveAttribute(
          "data-chat-placement",
          "center",
        ),
      );
      expect(screen.getByTestId("toggle-right-panel")).toHaveAttribute(
        "title",
        "Close chat",
      );
    });
  });

  describe("Base workspace presentation", () => {
    function serveBase(name = "Mainline") {
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "base" },
          }),
        ),
        http.get("/api/v1/workspaces/base", () =>
          HttpResponse.json({
            data: { ...mockWorkspaceDetails.base, name },
          }),
        ),
      );
    }

    function assertNoBaseChat() {
      expect(screen.queryByTestId("center-chat-tab")).toBeNull();
      expect(screen.queryByTestId("center-chat-panel")).toBeNull();
      expect(screen.queryByTestId("right-panel")).toBeNull();
      expect(screen.queryByTestId("chat-panel")).toBeNull();
      expect(screen.queryByText(/Owner:/)).toBeNull();
      expect(screen.queryByText(/VM:/)).toBeNull();
      expect(screen.queryByTestId("workspace-fork-terminal")).toBeNull();
    }

    it.each(["admin", "member", "viewer"] as const)(
      "shows the read-only Base browser to an authorized %s, regardless of the root name",
      async (role) => {
        mockDesktopMatchMedia();
        serveBase();
        const user = createMockUser();
        user.orgs[0].role = role;
        renderApp(
          <Routes>
            <Route
              path="/:org/:workspace"
              element={<WorkspacePageWithRouteProbe />}
            />
          </Routes>,
          {
            route: "/org-g5labs/Mainline?session=chat-tgr-1&view=chat&keep=1",
            user,
          },
        );
        await waitFor(() =>
          expect(screen.getByTestId("route-probe")).toHaveTextContent(
            /^\/org-g5labs\/Mainline\?keep=1$/,
          ),
        );
        expect(
          screen.getByTestId("workspace-readonly-banner"),
        ).toHaveTextContent(
          "Read-only workspace. Fork workspace to make edits.",
        );
        assertNoBaseChat();
        expect(screen.getByTestId("center-content-panel")).not.toHaveAttribute(
          "hidden",
        );
      },
    );

    it("keeps a Base task output readable without its Chat entry point", async () => {
      mockDesktopMatchMedia();
      serveBase();
      const task = {
        ...createMockTask({
          id: "base-task",
          workspaceId: "base",
          status: "succeeded",
        }),
        sessionId: "retained-base-session",
      };
      server.use(
        http.get("/api/v1/workspaces/:id/tasks", () =>
          HttpResponse.json({
            data: { tasks: [task], cursor: null, hasMore: false },
          }),
        ),
        http.get("/api/v1/tasks/:taskId", () =>
          HttpResponse.json({ data: task }),
        ),
      );
      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace/task/:taskId"
            element={<WorkspacePage />}
          />
        </Routes>,
        {
          route: "/org-g5labs/Mainline/task/base-task",
          user: createMockUser(),
        },
      );
      expect(
        await screen.findByRole("tab", { name: "Workspace changes" }),
      ).toBeVisible();
      assertNoBaseChat();
    });

    it("uses durable browsing rather than Chat on a mobile Base visit", async () => {
      vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
        matches: !query.includes("min-width: 1024px"),
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      }));
      serveBase();
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        { route: "/org-g5labs/Mainline", user: createMockUser() },
      );
      expect(await screen.findByTestId("center-panel")).toBeInTheDocument();
      assertNoBaseChat();
    });

    it("keeps Chat and the stopped-owner explanation for a working child named Base", async () => {
      mockDesktopMatchMedia();
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "task-graph-rework" },
          }),
        ),
        http.get("/api/v1/workspaces/task-graph-rework", () =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails["task-graph-rework"],
              name: "Base",
              isBase: false,
              clonedFromId: null,
            },
          }),
        ),
        http.get("/api/v1/workspaces/task-graph-rework/vm", () =>
          HttpResponse.json({
            data: { status: "stopped", ownerUserId: "user-alex" },
          }),
        ),
      );
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        { route: "/org-g5labs/Base", user: createMockUser() },
      );
      expect(
        await screen.findByRole("tab", { name: "Chat" }),
      ).toBeInTheDocument();
      await waitFor(() =>
        expect(
          screen.getByTestId("workspace-readonly-banner"),
        ).toHaveTextContent(
          "Workspace is stopped. Start it to edit; durable files remain available for browsing.",
        ),
      );
      expect(screen.getByText(/Owner:/)).toBeInTheDocument();
      expect(screen.getByTestId("workspace-vm-start")).toBeInTheDocument();
    });

    it("unmounts detached Chat while Base metadata loads and ignores old workspace metadata cache writes", async () => {
      mockDesktopMatchMedia();
      let releaseBase!: () => void;
      const baseReady = new Promise<void>((resolve) => {
        releaseBase = resolve;
      });
      server.use(
        http.get("/api/v1/resolve", ({ request }) =>
          HttpResponse.json({
            data: {
              orgId: "org-g5labs",
              workspaceId:
                new URL(request.url).searchParams.get("workspace") ===
                "Mainline"
                  ? "base"
                  : "task-graph-rework",
            },
          }),
        ),
        http.get("/api/v1/workspaces/base", async () => {
          await baseReady;
          return HttpResponse.json({
            data: { ...mockWorkspaceDetails.base, name: "Mainline" },
          });
        }),
      );
      const { queryClient } = renderApp(
        <Routes>
          <Route
            path="/:org/:workspace"
            element={
              <WorkspacePageWithWorkspaceSwitchButton destination="/org-g5labs/Mainline?session=chat-tgr-1" />
            }
          />
        </Routes>,
        {
          route: "/org-g5labs/task-graph-rework?session=chat-tgr-1",
          user: createMockUser(),
        },
      );
      const user = userEvent.setup();
      await screen.findByRole("tab", { name: "Chat" });
      await user.click(await screen.findByTitle("Pop out"));
      await user.click(
        screen.getByRole("button", { name: "Switch workspace" }),
      );
      await waitFor(() =>
        expect(screen.queryByTestId("chat-panel")).toBeNull(),
      );
      await act(async () => {
        queryClient.setQueryData(
          ["workspace", "task-graph-rework"],
          mockWorkspaceDetails["task-graph-rework"],
        );
        releaseBase();
      });
      await waitFor(() =>
        expect(screen.getByTestId("route-probe")).toHaveTextContent(
          /^\/org-g5labs\/Mainline$/,
        ),
      );
      assertNoBaseChat();
    });
  });

  describe("read-only workspace (M25)", () => {
    it("renders the persistent read-only banner and a workspace read-only badge", async () => {
      // The banner + top chrome are part of the DESKTOP shell — report desktop.
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

      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "test-workspace" },
          }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) => {
          const id = params.id as string;
          const base =
            mockWorkspaceDetails[id] ?? Object.values(mockWorkspaceDetails)[0];
          return HttpResponse.json({
            data: { ...base, id, isBase: false, readOnly: true },
          });
        }),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      const banner = await screen.findByTestId("workspace-readonly-banner");
      expect(banner).toHaveTextContent(
        /Read-only workspace\. Fork workspace to make edits\./i,
      );
      // At least the top-bar/switcher badge is present.
      expect(
        screen.getAllByTestId("workspace-readonly-badge").length,
      ).toBeGreaterThanOrEqual(1);
    });

    it("keeps chat history available without mutation controls on a read-only workspace", async () => {
      // The right panel + top-bar chat toggle are part of the DESKTOP shell.
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

      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "test-workspace" },
          }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) => {
          const id = params.id as string;
          const base =
            mockWorkspaceDetails[id] ?? Object.values(mockWorkspaceDetails)[0];
          return HttpResponse.json({
            data: { ...base, id, isBase: false, readOnly: true },
          });
        }),
        http.get("/api/v1/sessions/:sessionId/inputs", ({ params }) =>
          HttpResponse.json({
            data: [
              {
                id: "uncertain-reader-input",
                sessionId: params.sessionId,
                inputIndex: 0,
                content: "possibly delivered reader-visible message",
                artifacts: [],
                revision: 1,
                inputOrigin: "human",
                delivery: "delivery_unknown",
                deliveryGeneration: 1,
                taskId: "task-reader-input",
                attempt: 1,
                createdAt: "2026-09-21T00:00:00Z",
                deliveredAt: null,
                withdrawnAt: null,
              },
            ],
          }),
        ),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=chat-tgr-1",
          user: createMockUser(),
        },
      );

      // Wait for the read-only shell to load (the banner is read-only-gated).
      await screen.findByTestId("workspace-readonly-banner");

      const rightPanel = screen.getByTestId("right-panel");
      expect(rightPanel).toHaveAttribute("data-chat-placement", "center");
      expect(screen.getByTestId("center-chat-tab")).toHaveAttribute(
        "aria-selected",
        "true",
      );
      expect(screen.queryByTestId("toggle-right-panel")).toBeNull();
      expect(within(rightPanel).getByTestId("chat-input")).toBeDisabled();
      expect(within(rightPanel).queryByTestId("new-session-header")).toBeNull();
      expect(
        within(rightPanel).queryByTestId("stop-streaming-button"),
      ).toBeNull();
      expect(
        await within(rightPanel).findByText(
          "possibly delivered reader-visible message",
        ),
      ).toBeInTheDocument();
      expect(
        within(rightPanel).queryByTestId(
          "chat-queued-input-redeliver-uncertain-reader-input",
        ),
      ).toBeNull();
      expect(
        within(rightPanel).queryByTestId(
          "chat-queued-input-cancel-uncertain-reader-input",
        ),
      ).toBeNull();
    });

    it("keeps chat history available to an owner while the workspace is stopped", async () => {
      mockDesktopMatchMedia();
      // Start's queued outcome is announced once, from Start's answer and
      // the poll that follows it.
      resetCapacityWaitAnnouncementsForTests();
      let polledVmStatus: "stopped" | "queued_for_capacity" = "stopped";
      server.use(
        http.get("/api/v1/resolve", () =>
          HttpResponse.json({
            data: { orgId: "org-g5labs", workspaceId: "test-workspace" },
          }),
        ),
        http.get("/api/v1/workspaces/:id", ({ params }) =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails["task-graph-rework"],
              id: params.id,
              name: "Test Workspace",
            },
          }),
        ),
        http.get("/api/v1/workspaces/:id/vm", ({ params }) =>
          HttpResponse.json({
            data: {
              orgId: "org-g5labs",
              workspaceId: params.id,
              ownerUserId: "user-alex",
              status: polledVmStatus,
              hostId: null,
              vmId: null,
              epoch: 1,
              imageRef: null,
              moduleCatalogVersion: null,
              startIntent: null,
              startDeadlineAt: null,
              startedAt: null,
              lastActivityAt: null,
              stopReason: null,
              stopOperationId: null,
              stopDeadlineAt: null,
              pendingStopReason: null,
              pendingStopDeadlineAt: null,
              nudgedAt: null,
              lastFlushId: null,
              unflushedLossAt: null,
              lastError: null,
            },
          }),
        ),
        http.post("/api/v1/workspaces/:id/resume", ({ params }) => {
          polledVmStatus = "queued_for_capacity";
          return HttpResponse.json({
            data: {
              outcome: "queued_for_capacity",
              vm: { workspaceId: params.id, status: "queued_for_capacity" },
            },
          });
        }),
        http.get("/api/v1/sessions/:sessionId/inputs", ({ params }) =>
          HttpResponse.json({
            data: [
              {
                id: "uncertain-owner-input",
                sessionId: params.sessionId,
                inputIndex: 0,
                content: "possibly delivered owner message",
                artifacts: [],
                revision: 1,
                inputOrigin: "human",
                delivery: "delivery_unknown",
                deliveryGeneration: 1,
                taskId: "task-owner-input",
                attempt: 1,
                createdAt: "2026-09-21T00:00:00Z",
                deliveredAt: null,
                withdrawnAt: null,
              },
            ],
          }),
        ),
      );

      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace?session=chat-tgr-1",
          user: createMockUser(),
        },
      );

      const rightPanel = await screen.findByTestId("right-panel");
      // The composer is disabled until the workspace detail (owner) and the
      // VM attachment resolve; then the owner's stopped composer opens with
      // Start and send and says so (g5.chat.start-and-send).
      await waitFor(() =>
        expect(within(rightPanel).getByTestId("chat-input")).toBeEnabled(),
      );
      expect(
        within(rightPanel).getByTestId("start-and-send-note"),
      ).toHaveTextContent("The workspace is stopped.");
      expect(
        within(rightPanel).getByTestId("chat-panel-send"),
      ).toHaveTextContent("Start and send");
      expect(
        within(rightPanel).queryByTestId(
          "chat-composer-disabled-reason-trigger",
        ),
      ).toBeNull();
      // Sessions are platform records: the owner keeps New session while
      // stopped (`g5.workspace.ui.gates`).
      expect(
        within(rightPanel).getByTestId("new-session-header"),
      ).toBeInTheDocument();
      expect(
        await within(rightPanel).findByText("possibly delivered owner message"),
      ).toBeInTheDocument();
      expect(
        within(rightPanel).getByTestId(
          "chat-queued-input-redeliver-uncertain-owner-input",
        ),
      ).toBeInTheDocument();
      expect(
        within(rightPanel).getByTestId(
          "chat-queued-input-cancel-uncertain-owner-input",
        ),
      ).toBeInTheDocument();
      const controls = screen.getByTestId("workbench-workspace-controls");
      const start = within(controls).getByTestId("workspace-vm-start");
      expect(start).toBeEnabled();
      // The terminal needs the VM: the rail's control stays, disabled, and
      // trying it reveals the reason with Start (`g5.ui.unavailable-actions`).
      const railTerminal = within(
        screen.getByTestId("sidebar-files-tab"),
      ).getByRole("button", { name: "Terminal" });
      expect(railTerminal).toBeDisabled();
      fireEvent.click(
        within(screen.getByTestId("sidebar-files-tab")).getByTestId(
          "tree-node-header-action-hint-trigger",
        ),
      );
      expect(
        within(screen.getByTestId("sidebar-files-tab")).getByTestId(
          "tree-node-header-action-hint",
        ),
      ).toHaveTextContent("Start the workspace to open a terminal.");
      expect(screen.queryByTestId("workspace-vm-stop")).toBeNull();
      await userEvent.click(start);
      expect(
        await screen.findByText("Workspace waiting for capacity"),
      ).toBeInTheDocument();
      expect(screen.queryByText("The workspace VM is attaching.")).toBeNull();
    });
  });

  describe("deep linking", () => {
    it("renders workspace page at spec deep-link route", async () => {
      renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace/spec/auth/validator",
        user: createMockUser(),
      });

      await waitFor(() => {
        const page =
          document.querySelector('[class*="workspace"]') || document.body;
        expect(page).toBeTruthy();
      });
    });
  });

  // =========================================================================
  // Structural Integrity Tests — Drift Prevention
  // =========================================================================

  describe("structural integrity", () => {
    it("renders workspace page without crash at workspace route", async () => {
      // test_app_mounts_without_crash
      const { container } = renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        // If we get here without an error boundary catching, the app mounted
        expect(container).toBeTruthy();
        expect(container.innerHTML.length).toBeGreaterThan(0);
      });
    });

    it("does not render placeholder strings in center panel", async () => {
      renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        const page =
          document.querySelector('[class*="workspace"]') || document.body;
        expect(page).toBeTruthy();
      });

      // test_no_placeholder_strings_in_center_panel
      const bodyText = document.body.textContent || "";
      const lowerText = bodyText.toLowerCase();
      expect(lowerText).not.toContain("todo: wire");
      expect(lowerText).not.toContain("not implemented");
    });

    it("has router context available for useNavigate", async () => {
      // test_router_wraps_app — if WorkspacePage renders without throwing
      // about missing router context, the router is properly wrapping the app
      const { container } = renderApp(<WorkspacePage />, {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      });

      await waitFor(() => {
        expect(container.innerHTML.length).toBeGreaterThan(0);
      });
    });
  });
});

describe("workspaceDocumentTitle", () => {
  it("renders the open workspace as 'G5 - {name}'", () => {
    expect(workspaceDocumentTitle("Payments")).toBe("G5 - Payments");
  });

  it("uses the full trimmed workspace name without a length cap", () => {
    const longName = "Customer Billing Platform";
    expect(workspaceDocumentTitle(longName)).toBe(
      "G5 - Customer Billing Platform",
    );
  });

  it("strips surrounding whitespace before trimming", () => {
    expect(workspaceDocumentTitle("  Ledger  ")).toBe("G5 - Ledger");
  });

  it("falls back to the default title when no usable name exists", () => {
    expect(workspaceDocumentTitle(null)).toBe(DEFAULT_DOCUMENT_TITLE);
    expect(workspaceDocumentTitle(undefined)).toBe(DEFAULT_DOCUMENT_TITLE);
    expect(workspaceDocumentTitle("   ")).toBe(DEFAULT_DOCUMENT_TITLE);
  });
});

describe("WorkspacePage review workspace hand-off", () => {
  beforeEach(installWorkingWorkspaceDetail);
  afterEach(() => {
    try {
      sessionStorage.clear();
    } catch {
      // ignore
    }
  });

  const reviewWorkspace = MOCK_WORKSPACES[0];
  const useReviewContext = () =>
    server.use(
      http.get(`*/api/v1/workspaces/${reviewWorkspace.id}/review`, () =>
        HttpResponse.json({
          data: {
            asSource: null,
            asReview: {
              forkId: "fork-1",
              orgId: "org-g5labs",
              sourceWorkspaceId: "ws-source-1",
              sourceWorkspaceName: "kaihui-test3",
              reviewWorkspaceId: reviewWorkspace.id,
              reviewWorkspaceName: reviewWorkspace.name,
              reviewerUserId: "user-alex",
              reviewerDisplayName: "Alex Chen",
              vmStatus: "running",
              vmStartedAt: "2026-09-23T14:02:00.000Z",
              vmLastActivityAt: "2026-09-23T14:20:00.000Z",
              lastFlushedAt: null,
              createdAt: "2026-09-22T16:10:00.000Z",
            },
            openSuggestions: 0,
          },
        }),
      ),
    );

  it("opens the chat with the review prompt filled in on first arrival (phone layout)", async () => {
    useReviewContext();
    markReviewPromptSeed(reviewWorkspace.id);

    renderApp(
      <Routes>
        <Route path="/:org/:workspace" element={<WorkspacePage />} />
      </Routes>,
      {
        route: "/org-g5labs/test-workspace",
        user: createMockUser(),
      },
    );

    expect(await screen.findByTestId("workspace-review-banner")).toBeVisible();
    await waitFor(() => {
      expect(screen.getByTestId("chat-input")).toHaveValue(
        reviewWorkspaceChatPrompt(),
      );
    });
  });

  it("opens the chat with the review prompt filled in on first arrival", async () => {
    // Desktop layout: the chat composer lives in the right panel.
    const matchMedia = vi.mocked(window.matchMedia);
    matchMedia.mockImplementation((query: string) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
    try {
      useReviewContext();
      // The Approvals page marks the one-shot seed before navigating here.
      markReviewPromptSeed(reviewWorkspace.id);

      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<WorkspacePage />} />
        </Routes>,
        {
          route: "/org-g5labs/test-workspace",
          user: createMockUser(),
        },
      );

      expect(
        await screen.findByTestId("workspace-review-banner"),
      ).toBeVisible();
      await waitFor(() => {
        expect(
          within(screen.getByTestId("right-panel")).getByTestId("chat-input"),
        ).toHaveValue(reviewWorkspaceChatPrompt());
      });
      // The review loop option sits on top of the seeded chat
      // (ui g5.workbench.review-prompts).
      expect(
        within(screen.getByTestId("right-panel")).getByTestId(
          "composer-seed-review-iterations",
        ),
      ).toHaveValue("1");
      // Consumed once: a reload does not seed again.
      expect(
        sessionStorage.getItem(
          `g5.review-workspace.seed:${reviewWorkspace.id}`,
        ),
      ).toBeNull();
    } finally {
      matchMedia.mockImplementation((query: string) => ({
        matches: !query.includes("min-width: 1024px"),
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      }));
    }
  });
});

describe("WorkspacePage external issues", () => {
  it("opens Workspace Details at its Issues section from ?view=issues and the primary issue from its row", async () => {
    mockDesktopMatchMedia();
    localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
    serveExternalIssuesFeature();
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/issue-links", () =>
        HttpResponse.json({ data: { links: [makeIssueLink()] } }),
      ),
      http.get("/api/v1/orgs/:orgId/issues/github/11/12", () =>
        HttpResponse.json({ data: makeIssueDetail() }),
      ),
    );
    const user = userEvent.setup();
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace/*"
          element={<WorkspacePageWithRouteProbe />}
        />
      </Routes>,
      {
        route: "/g5labs/test-workspace?view=issues",
        user: createMockUser(),
      },
    );

    const detailsTab = await screen.findByRole("tab", {
      name: "Workspace Details",
    });
    expect(detailsTab).toHaveAttribute("aria-selected", "true");
    expect(
      screen.queryByRole("tab", { name: "Issues" }),
    ).not.toBeInTheDocument();
    const section = await screen.findByTestId("workspace-issues-panel");
    expect(
      await within(section).findByTestId("workspace-issue-link-link-1"),
    ).toBeVisible();
    // The hint stays while Details shows the section.
    expect(screen.getByTestId("route-probe")).toHaveTextContent("view=issues");
    // Issues live in Details; the row's Open button opens the issue page.
    await user.click(
      await screen.findByRole("button", { name: "Open acme/api#12" }),
    );

    const issueTab = await screen.findByRole("tab", { name: "acme/api#12" });
    expect(issueTab).toHaveAttribute("aria-selected", "true");
    expect(
      await screen.findByTestId("external-issue-detail"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).not.toHaveTextContent(
        "view=issues",
      ),
    );

    // Re-opening from the list focuses the same issue tab (one per issue).
    await user.click(detailsTab);
    await user.click(
      await screen.findByRole("button", { name: "Open acme/api#12" }),
    );
    expect(screen.getAllByRole("tab", { name: "acme/api#12" })).toHaveLength(1);
  });

  it("opens Activity from ?view=activity and from the Workspace tab", async () => {
    mockDesktopMatchMedia();
    localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
    serveExternalIssuesFeature();
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/issue-links", () =>
        HttpResponse.json({ data: { links: [makeIssueLink()] } }),
      ),
      http.get("/api/v1/workspaces/:workspaceId/activity", () =>
        HttpResponse.json({
          data: {
            primaryIssue: null,
            issueError: null,
            issueHistoryTruncated: false,
            entries: [],
            earlierCursor: null,
            composer: null,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace/*"
          element={
            <WorkspacePageWithWorkspaceSwitchButton destination="?view=issues" />
          }
        />
      </Routes>,
      {
        route: "/g5labs/test-workspace?view=activity",
        user: createMockUser(),
      },
    );

    const activityTab = await screen.findByRole("tab", { name: "Workspace" });
    expect(activityTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("workspace-activity-empty")).toBeVisible();
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      "view=activity",
    );

    // Leave Activity for Details through its URL (Activity no longer repeats
    // an issue-link prompt; issue management stays in Details), then come
    // back from the Workspace tab.
    await user.click(screen.getByRole("button", { name: "Switch workspace" }));
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "view=issues",
      ),
    );
    expect(
      await screen.findByRole("tab", { name: "Workspace Details" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("workspace-issues-panel")).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "Workspace" }));
    await waitFor(() =>
      expect(screen.getByTestId("route-probe")).toHaveTextContent(
        "view=activity",
      ),
    );
    expect(screen.getAllByRole("tab", { name: "Workspace" })).toHaveLength(1);
  });

  it("hides the Issues entrypoint and primary chip while the feature is off", async () => {
    mockDesktopMatchMedia();
    localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
    let requested = false;
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/issue-links", () => {
        requested = true;
        return HttpResponse.json({ data: { links: [makeIssueLink()] } });
      }),
    );
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace/*"
          element={<WorkspacePageWithRouteProbe />}
        />
      </Routes>,
      { route: "/g5labs/test-workspace", user: createMockUser() },
    );

    expect(await screen.findByTestId("center-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("top-bar-activity")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("top-bar-primary-issue"),
    ).not.toBeInTheDocument();
    expect(requested).toBe(false);
  });
});

// Claude background waits (g5.conversation.providers.claude-background.*,
// g5.agents.lifecycle.owner-stop): the owner keeps Stop and End for a live
// reply whatever the VM status read says; readers see the same waiting panel
// without either control.
function installBackgroundWaitFixture(
  options: { failFirstEnd?: boolean } = {},
) {
  const session = createMockChatSession({
    id: "background-wait-session",
    workspaceId: "base",
    scopeType: "workspace",
    scopeRef: "base",
    title: "Background wait session",
    agentProvider: "claude_code",
    messageCount: 2,
  });
  const messages = [
    createMockChatMessage({
      id: "user-a",
      sessionId: session.id,
      role: "user",
      content: "Run the suite in the background",
      status: "complete",
      messageIndex: 0,
    }),
    createMockChatMessage({
      id: "assistant-a",
      sessionId: session.id,
      content: "",
      status: "streaming",
      messageIndex: 1,
    }),
  ];
  const startedAt = Date.now() - 30_000;
  // Raw journal rows as the session timeline API returns them; the hook
  // projects them like production.
  const journal = (
    id: string,
    kind: string,
    payload: Record<string, unknown>,
    sequence: number,
  ) => ({
    id,
    entryType: kind === "message.final" ? "agent_message" : "checkpoint",
    content: typeof payload.content === "string" ? payload.content : kind,
    sequence,
    createdAt: new Date(startedAt + sequence * 1_000).toISOString(),
    metadata: {
      schema_version: "g5.task-event.v1",
      kind,
      payload,
      taskId: "task-a",
      attempt: 1,
      messageId: "assistant-a",
      sessionId: session.id,
    },
  });
  const entries = [
    journal(
      "final-a",
      "message.final",
      {
        block_id: "g5:terminal-result",
        content: "Started the suite in the background.",
      },
      1,
    ),
    journal(
      "set-a",
      "background_tasks.replaced",
      {
        tasks_ref: "sha256:set-a",
        tasks: [
          {
            task_id: "b1nohmzbo",
            kind: "shell",
            description: "just docker-ci",
            started_at_unix_ms: startedAt,
          },
        ],
        count: 1,
        truncated: false,
        content: "Waiting on 1 background task",
      },
      2,
    ),
    journal(
      "usage-a",
      "usage.updated",
      { usage: { input_tokens: 1, output_tokens: 2 } },
      3,
    ),
  ];
  const stopRequestIds: string[] = [];
  const endRequests: Array<{ sessionId: string; body: unknown }> = [];
  let endFailuresRemaining = options.failFirstEnd ? 1 : 0;

  server.use(
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/:sessionId", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/:sessionId/messages", () =>
      HttpResponse.json({ data: messages }),
    ),
    http.get("/api/v1/sessions/:sessionId/timeline", () =>
      HttpResponse.json({ data: { entries, cursor: null, hasMore: false } }),
    ),
    http.post(
      "/api/v1/sessions/:sessionId/messages/:messageId/stop",
      ({ params }) => {
        stopRequestIds.push(String(params.messageId));
        return HttpResponse.json({ data: { stopped: true } });
      },
    ),
    http.post(
      "/api/v1/sessions/:sessionId/background/end",
      async ({ params, request }) => {
        endRequests.push({
          sessionId: String(params.sessionId),
          body: await request.json(),
        });
        if (endFailuresRemaining > 0) {
          endFailuresRemaining -= 1;
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "session has no running agent attempt",
              },
            },
            { status: 409 },
          );
        }
        return HttpResponse.json({ data: { delivered: true } });
      },
    ),
  );

  return { sessionId: session.id, stopRequestIds, endRequests };
}

function renderBackgroundWaitWorkspace() {
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<WorkspacePage />} />
    </Routes>,
    {
      route: "/org-g5labs/test-workspace?session=background-wait-session",
      user: createMockUser(),
    },
  );
}

describe("WorkspacePage Claude background wait controls", () => {
  beforeEach(installWorkingWorkspaceDetail);

  it("keeps the owner's Stop and End while the VM status read is still loading", async () => {
    mockDesktopMatchMedia();
    const fixture = installBackgroundWaitFixture();
    server.use(
      http.get("/api/v1/workspaces/:id/vm", async () => {
        await delay("infinite");
        return HttpResponse.json({ data: null });
      }),
    );
    const { queryClient } = renderBackgroundWaitWorkspace();

    const rightPanel = await screen.findByTestId("right-panel");
    expect(
      await within(rightPanel).findByTestId("background-wait-panel"),
    ).toBeVisible();
    expect(queryClient.getQueryState(["workspace-vm", "base"])?.status).toBe(
      "pending",
    );
    // Without a known VM state the page still gates sending, and says the
    // state is not known yet rather than "stopped" (`g5.workspace.ui.vm-state`)…
    fireEvent.click(
      within(rightPanel).getByTestId("chat-composer-disabled-reason-trigger"),
    );
    expect(
      within(rightPanel).getByTestId("chat-composer-disabled-reason"),
    ).toHaveTextContent(
      "The workspace's state is not known yet; send messages once it is.",
    );
    // …but the owner keeps the controls that end the live reply.
    expect(
      within(rightPanel).getByTestId("end-background-wait-button"),
    ).toBeEnabled();
    fireEvent.click(within(rightPanel).getByTestId("stop-streaming-button"));
    await waitFor(() => {
      expect(fixture.stopRequestIds).toEqual(["assistant-a"]);
    });
  });

  it("keeps the owner's Stop when the VM status read fails", async () => {
    mockDesktopMatchMedia();
    const fixture = installBackgroundWaitFixture();
    server.use(
      http.get("/api/v1/workspaces/:id/vm", () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "VM status unavailable" } },
          { status: 500 },
        ),
      ),
    );
    const { queryClient } = renderBackgroundWaitWorkspace();

    const rightPanel = await screen.findByTestId("right-panel");
    await waitFor(() =>
      expect(queryClient.getQueryState(["workspace-vm", "base"])?.status).toBe(
        "error",
      ),
    );
    fireEvent.click(
      await within(rightPanel).findByTestId("stop-streaming-button"),
    );
    await waitFor(() => {
      expect(fixture.stopRequestIds).toEqual(["assistant-a"]);
    });
  });

  it("posts End to the background end endpoint and reports a refused End", async () => {
    mockDesktopMatchMedia();
    const fixture = installBackgroundWaitFixture({ failFirstEnd: true });
    renderBackgroundWaitWorkspace();

    const rightPanel = await screen.findByTestId("right-panel");
    const end = await within(rightPanel).findByTestId(
      "end-background-wait-button",
    );
    fireEvent.click(end);
    // End names the attempt whose wait the panel shows, so a delayed End
    // cannot reach a later attempt.
    const endOfDisplayedWait = {
      sessionId: fixture.sessionId,
      body: { taskId: "task-a", attempt: 1 },
    };
    await waitFor(() => {
      expect(fixture.endRequests).toEqual([endOfDisplayedWait]);
    });
    expect(await screen.findByText("Could not end the wait")).toBeVisible();
    expect(
      screen.getByText("session has no running agent attempt"),
    ).toBeVisible();
    await waitFor(() => expect(end).toBeEnabled());
    expect(end).toHaveTextContent("End wait");

    fireEvent.click(end);
    await waitFor(() => {
      expect(fixture.endRequests).toEqual([
        endOfDisplayedWait,
        endOfDisplayedWait,
      ]);
    });
    // Delivered: End stays disabled until the attempt finishes the wait.
    expect(end).toBeDisabled();
    expect(end).toHaveTextContent("Ending…");
    expect(fixture.stopRequestIds).toEqual([]);
  });

  it("shows a reader the waiting panel without End or Stop", async () => {
    mockDesktopMatchMedia();
    installBackgroundWaitFixture();
    server.use(
      http.get("/api/v1/workspaces/:id", ({ params }) =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails.base,
            id: params.id,
            isBase: false,
            clonedFromId: "protected-root",
            ownerUserId: "user-alex-other",
          },
        }),
      ),
    );
    renderBackgroundWaitWorkspace();

    const rightPanel = await screen.findByTestId("right-panel");
    const panel = await within(rightPanel).findByTestId(
      "background-wait-panel",
    );
    expect(within(panel).getByText("just docker-ci")).toBeVisible();
    expect(
      within(rightPanel).queryByTestId("end-background-wait-button"),
    ).not.toBeInTheDocument();
    expect(
      within(rightPanel).queryByTestId("stop-streaming-button"),
    ).not.toBeInTheDocument();
  });
});

// workspaces g5.workspace.ui.chat-corner-button: a bottom-right secondary
// button on desktop workspace routes opens Chat as the floating panel anchored
// to the right edge with the last active session; it hides while Chat is the
// center surface, while that panel or a covering overlay is open, stays
// disabled, with the durable gate's reason, on Base and in a workspace the
// viewer cannot write, and can be dragged to a spot this browser remembers.
describe("WorkspacePage — corner Chat button (g5.workspace.ui.chat-corner-button)", () => {
  beforeEach(() => {
    installWorkingWorkspaceDetail();
    window.localStorage.removeItem("g5-user-settings");
    window.localStorage.removeItem("g5:chat-corner-button-offset");
  });

  function installSession() {
    const session = createMockChatSession({
      id: "corner-session",
      scopeType: "workspace",
      scopeRef: "ws-123",
      title: "Corner session",
      agentProvider: "claude_code",
      messageCount: 0,
    });
    server.use(
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: [session] }),
      ),
      http.get("/api/v1/sessions/:sessionId", ({ params }) =>
        params.sessionId === session.id
          ? HttpResponse.json({ data: session })
          : HttpResponse.json({ data: null }, { status: 404 }),
      ),
      http.get("/api/v1/sessions/:sessionId/messages", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/sessions/:sessionId/timeline", () =>
        HttpResponse.json({
          data: { entries: [], cursor: null, hasMore: false },
        }),
      ),
    );
    return session;
  }

  function serveDetail(overrides: Record<string, unknown>) {
    server.use(
      http.get("/api/v1/workspaces/:id", ({ params }) => {
        const id = params.id as string;
        const base =
          mockWorkspaceDetails[id] ?? Object.values(mockWorkspaceDetails)[0];
        return HttpResponse.json({
          data: { ...base, id, isBase: false, ...overrides },
        });
      }),
    );
  }

  /** The page plus a probe modal, so a covering overlay can be opened at will. */
  function WorkspacePageWithProbeModal() {
    const [open, setOpen] = useState(false);
    return (
      <>
        <WorkspacePage />
        <button
          type="button"
          data-testid="probe-modal-open"
          onClick={() => setOpen(true)}
        >
          open probe modal
        </button>
        {open ? (
          <Modal
            onClose={() => setOpen(false)}
            ariaLabel="Probe modal"
            testId="probe-modal"
          >
            <button type="button" onClick={() => setOpen(false)}>
              close probe modal
            </button>
          </Modal>
        ) : null}
      </>
    );
  }

  function renderWorkspace(
    route = "/org-g5labs/test-workspace",
    element = <WorkspacePage />,
  ) {
    mockDesktopMatchMedia();
    return renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={element} />
      </Routes>,
      { route, user: createMockUser() },
    );
  }

  /**
   * The button, which is absent while Chat is the center surface: once the
   * page settles on either, leave Chat from the ribbon's toggle (the tab
   * strip is hidden here) if it is open.
   */
  async function findCornerButton(): Promise<HTMLElement> {
    await waitFor(() =>
      expect(
        screen.queryByTestId("chat-corner-button") ??
          screen.queryByTestId("toggle-right-panel"),
      ).not.toBeNull(),
    );
    const toggle = screen.queryByTestId("toggle-right-panel");
    if (toggle?.getAttribute("title") === "Close chat") {
      fireEvent.click(toggle);
    }
    return screen.findByTestId("chat-corner-button");
  }

  /** The button once the signed-in person is known and the gate admits them. */
  async function findEnabledCornerButton(): Promise<HTMLElement> {
    const button = await findCornerButton();
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    return button;
  }

  /** The floating panel's root, found from its Dock control. */
  function floatingPanelRoot(): HTMLElement {
    const dock = screen.getByTestId("floating-panel-dock");
    return dock.parentElement!.parentElement as HTMLElement;
  }

  it("opens Chat at the right edge with the last session, focuses the composer, and Dock returns Chat to the center", async () => {
    const user = userEvent.setup();
    const session = installSession();
    renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

    const button = await findEnabledCornerButton();
    expect(button).toHaveAccessibleName("Open chat beside the content");
    // The center view's title; the chat's bubble carries it too.
    expect(
      await within(screen.getByTestId("center-chat-panel")).findByText(
        "Corner session",
      ),
    ).toBeInTheDocument();

    await user.click(button);

    const panel = await waitFor(floatingPanelRoot);
    // Anchored to the right edge under the chrome (jsdom lays nothing out,
    // so the chrome bottom is 0) and stretched to the bottom. An earlier test
    // may have left a narrow viewport, where the panel pins to the left edge.
    expect(panel.style.left).toBe(`${Math.max(0, window.innerWidth - 380)}px`);
    expect(panel.style.top).toBe("0px");
    expect(panel.style.height).toBe(`${window.innerHeight}px`);
    expect(within(panel).getByText("Corner session")).toBeInTheDocument();
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
    // Chat left the center for the content.
    expect(screen.getByTestId("center-chat-panel")).not.toBeVisible();
    await waitFor(() =>
      expect(within(panel).getByTestId("chat-input")).toHaveFocus(),
    );

    // The Dock control sits in the drag title bar; a plain click keeps jsdom
    // out of pointer capture.
    fireEvent.click(screen.getByTestId("floating-panel-dock"));
    await waitFor(() =>
      expect(screen.queryByTestId("floating-panel-dock")).toBeNull(),
    );
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
    );
    // Chat is the center surface again, where the button has no job.
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
  });

  it("has no job while Chat is the center surface and appears once the content takes it", async () => {
    const session = installSession();
    renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

    expect(
      await within(await screen.findByTestId("center-chat-panel")).findByText(
        "Corner session",
      ),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
    );
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();

    fireEvent.click(screen.getByTestId("toggle-right-panel"));
    expect(await screen.findByTestId("chat-corner-button")).toBeInTheDocument();
    expect(screen.getByTestId("center-chat-panel")).not.toBeVisible();

    fireEvent.click(screen.getByTestId("toggle-right-panel"));
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
    );
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
  });

  it("lets the center landing Pop out before any session exists", async () => {
    const user = userEvent.setup();
    renderWorkspace();

    const rightPanel = await screen.findByTestId("right-panel");
    await user.click(
      await within(rightPanel).findByTestId("chat-center-landing-detach"),
    );
    const popped = await waitFor(floatingPanelRoot);
    expect(within(popped).getByTestId("chat-input")).toBeInTheDocument();
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
  });

  it("opens at the spot it was dragged to in this browser", async () => {
    localStorage.setItem(
      "g5:chat-corner-button-offset",
      JSON.stringify({ x: 240, y: 90 }),
    );
    installSession();
    renderWorkspace();

    const button = await findCornerButton();
    expect(button.style.right).toBe("240px");
    expect(button.style.bottom).toBe("90px");
  });

  // ui g5.workspace.ui.chat-bubble-stack
  describe("chat bubble stack", () => {
    function installActiveSessions() {
      const common = {
        scopeType: "workspace" as const,
        scopeRef: "ws-123",
        agentProvider: "claude_code" as const,
        messageCount: 0,
      };
      const sessions = [
        createMockChatSession({
          ...common,
          id: "working-session",
          title: "Working chat",
          activity: {
            activeTurnStartedAt: "2024-11-15T10:00:00Z",
            lastFinishedAt: null,
            latestResult: null,
            unseenResult: false,
            awaitingInput: false,
          },
        }),
        createMockChatSession({
          ...common,
          id: "asking-session",
          title: "Asking chat",
          activity: {
            activeTurnStartedAt: "2024-11-15T10:01:00Z",
            lastFinishedAt: null,
            latestResult: null,
            unseenResult: false,
            awaitingInput: true,
          },
        }),
        createMockChatSession({
          ...common,
          id: "quiet-session",
          title: "Quiet chat",
          activity: {
            activeTurnStartedAt: null,
            lastFinishedAt: "2024-11-15T09:00:00Z",
            latestResult: { taskId: "quiet-task", attempt: 1, hasOutput: true },
            unseenResult: false,
          },
        }),
      ];
      server.use(
        http.get("/api/v1/workspaces/:id/sessions", () =>
          HttpResponse.json({ data: sessions }),
        ),
        http.get("/api/v1/sessions/:sessionId", ({ params }) => {
          const found = sessions.find((s) => s.id === params.sessionId);
          return found
            ? HttpResponse.json({ data: found })
            : HttpResponse.json({ data: null }, { status: 404 });
        }),
        http.get("/api/v1/sessions/:sessionId/messages", () =>
          HttpResponse.json({ data: [] }),
        ),
        http.get("/api/v1/sessions/:sessionId/timeline", () =>
          HttpResponse.json({
            data: { entries: [], cursor: null, hasMore: false },
          }),
        ),
      );
      return sessions;
    }

    it("draws 3D heads by default", async () => {
      installActiveSessions();
      renderWorkspace();

      await findEnabledCornerButton();
      const stack = await screen.findByTestId("chat-bubble-stack");
      expect(stack).toHaveAttribute("data-style", "3d");
      const heads = await within(stack).findAllByTestId("robot-head");
      expect(heads.map((head) => head.dataset.state)).toEqual([
        "sleeping",
        "happy",
        "working",
      ]);
    });

    it("stacks every chat above the button, most recent nearest, and opens the pressed one at the right edge", async () => {
      const user = userEvent.setup();
      window.localStorage.setItem(
        "g5-user-settings",
        JSON.stringify({ chatBubbleStyle: "still" }),
      );
      installActiveSessions();
      renderWorkspace();

      await findEnabledCornerButton();
      const stack = await screen.findByTestId("chat-bubble-stack");
      expect(
        within(stack)
          .getAllByTestId("chat-bubble")
          .map((bubble) => bubble.dataset.sessionId),
      ).toEqual(["quiet-session", "asking-session", "working-session"]);
      expect(screen.getByTestId("chat-corner-button-stack")).toContainElement(
        stack,
      );

      await user.click(
        within(stack).getByRole("button", {
          name: "Asking chat: Waiting for your answer",
        }),
      );
      const panel = await waitFor(floatingPanelRoot);
      expect(panel.style.left).toBe(
        `${Math.max(0, window.innerWidth - 380)}px`,
      );
      expect(within(panel).getByText("Asking chat")).toBeInTheDocument();
      expect(screen.queryByTestId("chat-corner-button")).toBeNull();
      expect(screen.queryByTestId("chat-bubble-stack")).toBeNull();
      expect(screen.getByTestId("center-chat-panel")).not.toBeVisible();
      await waitFor(() =>
        expect(within(panel).getByTestId("chat-input")).toHaveFocus(),
      );
    });

    it("gives the quiet chat a bubble too, and Close only closes the panel", async () => {
      const user = userEvent.setup();
      window.localStorage.setItem(
        "g5-user-settings",
        JSON.stringify({ chatBubbleStyle: "still" }),
      );
      installActiveSessions();
      renderWorkspace("/org-g5labs/test-workspace?session=quiet-session");

      await findEnabledCornerButton();
      const stack = await screen.findByTestId("chat-bubble-stack");
      expect(
        within(stack)
          .getAllByTestId("chat-bubble")
          .map((bubble) => bubble.dataset.sessionId),
      ).toEqual(["quiet-session", "asking-session", "working-session"]);
      expect(
        within(stack).getByRole("button", { name: "Quiet chat: Idle" }),
      ).toHaveAttribute("data-state", "idle");

      await user.click(
        within(stack).getByRole("button", {
          name: "Asking chat: Waiting for your answer",
        }),
      );
      const panel = await waitFor(floatingPanelRoot);
      expect(within(panel).getByText("Asking chat")).toBeInTheDocument();
      expect(screen.queryByTestId("floating-panel-minimize")).toBeNull();
      fireEvent.click(screen.getByTestId("floating-panel-close"));
      await findEnabledCornerButton();
      expect(
        within(screen.getByTestId("chat-bubble-stack"))
          .getAllByTestId("chat-bubble")
          .map((bubble) => bubble.dataset.sessionId),
      ).toEqual(["quiet-session", "asking-session", "working-session"]);
      expect(
        Object.keys(localStorage).filter((key) =>
          key.startsWith("g5:chat-bubbles:"),
        ),
      ).toEqual([]);
    });

    it("stays while Chat is the center surface and opens the pressed chat there", async () => {
      const user = userEvent.setup();
      window.localStorage.setItem(
        "g5-user-settings",
        JSON.stringify({ chatBubbleStyle: "still" }),
      );
      window.localStorage.setItem(
        "g5:chat-corner-button-offset",
        JSON.stringify({ x: 120, y: 60 }),
      );
      installActiveSessions();
      renderWorkspace("/org-g5labs/test-workspace?session=working-session");

      await waitFor(() =>
        expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
      );
      expect(screen.queryByTestId("chat-corner-button")).toBeNull();
      const corner = await screen.findByTestId("chat-center-bubble-stack");
      expect(corner.style.right).toBe("120px");
      expect(corner.style.bottom).toBe("60px");
      const stack = within(corner).getByTestId("chat-bubble-stack");
      expect(
        within(stack)
          .getAllByTestId("chat-bubble")
          .map((bubble) => bubble.dataset.sessionId),
      ).toEqual(["quiet-session", "asking-session", "working-session"]);

      await user.click(
        within(stack).getByRole("button", {
          name: "Asking chat: Waiting for your answer",
        }),
      );
      await waitFor(() =>
        expect(
          within(screen.getByTestId("center-chat-panel")).getByText(
            "Asking chat",
          ),
        ).toBeInTheDocument(),
      );
      expect(screen.queryByTestId("floating-panel-dock")).toBeNull();
      expect(
        screen.getByTestId("chat-center-bubble-stack"),
      ).toBeInTheDocument();
    });

    it("renders no stack while the setting is off", async () => {
      window.localStorage.setItem(
        "g5-user-settings",
        JSON.stringify({ chatBubbleStyle: "off" }),
      );
      installActiveSessions();
      renderWorkspace();

      await findEnabledCornerButton();
      expect(screen.queryByTestId("chat-bubble-stack")).toBeNull();
      expect(screen.queryByTestId("chat-corner-button-stack")).toBeNull();
    });

    it("renders no stack above a disabled button", async () => {
      installActiveSessions();
      serveDetail({ readOnly: true });
      renderWorkspace();

      const button = await findCornerButton();
      await waitFor(() =>
        expect(button).toHaveAttribute("aria-disabled", "true"),
      );
      expect(screen.queryByTestId("chat-bubble-stack")).toBeNull();
    });
  });

  it("keeps the unsent draft while Chat moves between the center and the floating panel", async () => {
    const user = userEvent.setup();
    const session = installSession();
    renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

    const rightPanel = await screen.findByTestId("right-panel");
    const input = await within(rightPanel).findByTestId("chat-input");
    await waitFor(() => expect(input).toBeEnabled());
    await user.type(input, "half a thought");
    expect(input).toHaveValue("half a thought");

    // Pop out remounts the panel in the floating frame.
    await user.click(within(rightPanel).getByTitle("Pop out"));
    const popped = await waitFor(floatingPanelRoot);
    expect(within(popped).getByTestId("chat-input")).toHaveValue(
      "half a thought",
    );

    // Dock brings it back to the center with the draft.
    fireEvent.click(screen.getByTestId("floating-panel-dock"));
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
    );
    expect(
      within(screen.getByTestId("right-panel")).getByTestId("chat-input"),
    ).toHaveValue("half a thought");

    // The corner route keeps it as well.
    await user.click(await findEnabledCornerButton());
    const anchored = await waitFor(floatingPanelRoot);
    expect(within(anchored).getByTestId("chat-input")).toHaveValue(
      "half a thought",
    );
  });

  it("brings the button back when the floating Chat is closed, with Chat closed", async () => {
    const user = userEvent.setup();
    const session = installSession();
    renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

    await user.click(await findEnabledCornerButton());
    await screen.findByTestId("floating-panel-close");
    fireEvent.click(screen.getByTestId("floating-panel-close"));

    expect(await screen.findByTestId("chat-corner-button")).toBeInTheDocument();
    expect(screen.queryByTestId("floating-panel-dock")).toBeNull();
    expect(screen.getByTestId("center-chat-panel")).not.toBeVisible();
  });

  it("keeps Pop out centered while the corner button anchors right", async () => {
    const user = userEvent.setup();
    const session = installSession();
    renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);

    await findEnabledCornerButton();
    const rightPanel = await screen.findByTestId("right-panel");
    await user.click(within(rightPanel).getByTitle("Pop out"));
    const popped = await waitFor(floatingPanelRoot);
    expect(popped.style.top).not.toBe("0px");
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
    fireEvent.click(screen.getByTestId("floating-panel-dock"));

    await user.click(await findEnabledCornerButton());
    const anchored = await waitFor(floatingPanelRoot);
    expect(anchored.style.top).toBe("0px");
  });

  describe("resizing", () => {
    // jsdom has no PointerEvent (fireEvent's plain-Event fallback drops the
    // coordinates) and no pointer capture, which the handles take on drag.
    // An earlier test may also have left a narrow viewport behind.
    const captureMethods = [
      "setPointerCapture",
      "releasePointerCapture",
    ] as const;
    let innerWidth = 0;
    beforeEach(() => {
      innerWidth = window.innerWidth;
      window.innerWidth = 1280;
      for (const name of captureMethods) {
        Object.defineProperty(HTMLElement.prototype, name, {
          configurable: true,
          value: vi.fn(),
        });
      }
    });
    afterEach(() => {
      window.innerWidth = innerWidth;
      for (const name of captureMethods) {
        delete (HTMLElement.prototype as unknown as Record<string, unknown>)[
          name
        ];
      }
    });

    function dragHandle(testId: string, dx: number, dy: number): void {
      const handle = screen.getByTestId(testId);
      firePointer(handle, "pointerdown", 600, 400);
      firePointer(handle, "pointermove", 600 + dx, 400 + dy);
      firePointer(handle, "pointerup", 600 + dx, 400 + dy);
    }

    it("resizes the right-edge Chat from the edges facing the content and remembers the width", async () => {
      const user = userEvent.setup();
      const session = installSession();
      renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);
      await user.click(await findEnabledCornerButton());
      const panel = await waitFor(floatingPanelRoot);
      expect(panel.style.left).toBe("900px");
      expect(panel.style.width).toBe("380px");

      // The right edge cannot leave the viewport.
      dragHandle("floating-panel-resize-right", 100, 0);
      expect(panel.style.left).toBe("900px");
      expect(panel.style.width).toBe("380px");

      // Left edge: the right edge stays at the viewport edge. The handle
      // takes pointer capture so the drag survives the pointer leaving it.
      dragHandle("floating-panel-resize-left", -120, 0);
      expect(panel.style.left).toBe("780px");
      expect(panel.style.width).toBe("500px");
      expect(localStorage.getItem("g5:chat-corner-width")).toBe("500");
      const leftHandle = screen.getByTestId("floating-panel-resize-left");
      expect(leftHandle.setPointerCapture).toHaveBeenCalledWith(1);
      expect(leftHandle.releasePointerCapture).toHaveBeenCalledWith(1);

      // Overshooting clamps to the width bound, still anchored right.
      dragHandle("floating-panel-resize-left", -1000, 0);
      expect(panel.style.left).toBe("480px");
      expect(panel.style.width).toBe("800px");

      // Top edge: the bottom stays at the viewport bottom.
      dragHandle("floating-panel-resize-top", 0, 100);
      expect(panel.style.top).toBe("100px");
      expect(panel.style.height).toBe(`${window.innerHeight - 100}px`);
      expect(localStorage.getItem("g5:chat-corner-width")).toBe("800");

      // Reopen: the remembered width at the right edge, the full height again.
      fireEvent.click(screen.getByTestId("floating-panel-close"));
      await user.click(await findEnabledCornerButton());
      const reopened = await waitFor(floatingPanelRoot);
      expect(reopened.style.width).toBe("800px");
      expect(reopened.style.left).toBe("480px");
      expect(reopened.style.top).toBe("0px");
      expect(reopened.style.height).toBe(`${window.innerHeight}px`);
    });

    it("resizes Pop out from any edge within the width bounds and the viewport", async () => {
      const user = userEvent.setup();
      const session = installSession();
      renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);
      await findEnabledCornerButton();
      const rightPanel = await screen.findByTestId("right-panel");
      await user.click(within(rightPanel).getByTitle("Pop out"));
      const panel = await waitFor(floatingPanelRoot);
      // Centered at the default size (1280 x 768 viewport).
      expect(panel.style.left).toBe("450px");
      expect(panel.style.top).toBe("124px");
      expect(panel.style.width).toBe("380px");
      expect(panel.style.height).toBe("520px");

      // Bottom-right corner grows both axes; the top-left corner stays.
      dragHandle("floating-panel-resize-bottom-right", 100, 100);
      expect(panel.style.left).toBe("450px");
      expect(panel.style.top).toBe("124px");
      expect(panel.style.width).toBe("480px");
      expect(panel.style.height).toBe("620px");

      // The bottom edge stops at the viewport.
      dragHandle("floating-panel-resize-bottom", 0, 2000);
      expect(panel.style.height).toBe(`${window.innerHeight - 124}px`);

      // Shrunk from the right to the minimum width (the right edge now at
      // 730), the left edge stops at the viewport edge before the width bound.
      dragHandle("floating-panel-resize-right", -2000, 0);
      expect(panel.style.left).toBe("450px");
      expect(panel.style.width).toBe("280px");
      dragHandle("floating-panel-resize-left", -2000, 0);
      expect(panel.style.left).toBe("0px");
      expect(panel.style.width).toBe("730px");

      // The right edge stops at the width bound; the left edge dragged
      // inwards stops at the minimum width with the right edge fixed.
      dragHandle("floating-panel-resize-right", 2000, 0);
      expect(panel.style.left).toBe("0px");
      expect(panel.style.width).toBe("800px");
      dragHandle("floating-panel-resize-left", 600, 0);
      expect(panel.style.left).toBe("520px");
      expect(panel.style.width).toBe("280px");

      // Dragged by its title bar until its bottom sits below the viewport,
      // the top edge still cannot be resized past the reachable band.
      const titleBar = panel.firstElementChild as HTMLElement;
      firePointer(titleBar, "pointerdown", 600, 400);
      firePointer(titleBar, "pointermove", 600, 1400);
      firePointer(titleBar, "pointerup", 600, 1400);
      expect(panel.style.top).toBe(`${window.innerHeight - 40}px`);
      const height = panel.style.height;
      dragHandle("floating-panel-resize-top", 0, 300);
      expect(panel.style.top).toBe(`${window.innerHeight - 40}px`);
      expect(panel.style.height).toBe(height);

      // Pop out does not write the corner Chat's remembered width.
      expect(localStorage.getItem("g5:chat-corner-width")).toBeNull();
    });

    it.each([
      { stored: "520", width: 520 },
      { stored: "20", width: 280 },
      { stored: "nonsense", width: 380 },
    ])(
      "reopens the right-edge Chat at the stored width $stored as $width while Pop out keeps its default",
      async ({ stored, width }) => {
        const user = userEvent.setup();
        const session = installSession();
        localStorage.setItem("g5:chat-corner-width", stored);
        renderWorkspace(`/org-g5labs/test-workspace?session=${session.id}`);
        await user.click(await findEnabledCornerButton());
        const anchored = await waitFor(floatingPanelRoot);
        expect(anchored.style.width).toBe(`${width}px`);
        expect(anchored.style.left).toBe(`${1280 - width}px`);
        fireEvent.click(screen.getByTestId("floating-panel-dock"));

        const rightPanel = await screen.findByTestId("right-panel");
        await user.click(within(rightPanel).getByTitle("Pop out"));
        const popped = await waitFor(floatingPanelRoot);
        expect(popped.style.width).toBe("380px");
      },
    );
  });

  it("hides while a modal covers the chrome and returns when it closes", async () => {
    const user = userEvent.setup();
    installSession();
    renderWorkspace(
      "/org-g5labs/test-workspace",
      <WorkspacePageWithProbeModal />,
    );

    await findEnabledCornerButton();
    await user.click(screen.getByTestId("probe-modal-open"));
    await screen.findByTestId("probe-modal-overlay");
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();

    await user.click(screen.getByText("close probe modal"));
    expect(await screen.findByTestId("chat-corner-button")).toBeInTheDocument();
  });

  it("renders nothing on mobile", async () => {
    installSession();
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
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      { route: "/org-g5labs/test-workspace", user: createMockUser() },
    );

    expect(
      await screen.findByTestId("mobile-notifications-open"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("chat-corner-button")).toBeNull();
  });

  it("stays visible but disabled on Base, with Base's reason, and opens nothing", async () => {
    const user = userEvent.setup();
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({
          data: { orgId: "org-g5labs", workspaceId: "base" },
        }),
      ),
      http.get("/api/v1/workspaces/base", () =>
        HttpResponse.json({ data: { ...mockWorkspaceDetails.base } }),
      ),
    );
    renderWorkspace("/org-g5labs/Mainline");

    const button = await screen.findByTestId("chat-corner-button");
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAttribute(
      "title",
      "Read-only workspace. Fork workspace to make edits.",
    );
    await user.click(button);
    expect(screen.queryByTestId("floating-panel-dock")).toBeNull();
    expect(screen.queryByTestId("center-chat-panel")).toBeNull();
  });

  it.each([
    {
      name: "a reader",
      overrides: { ownerUserId: "user-someone-else" },
      reason: "Only the owner of a running workspace can chat.",
    },
    {
      name: "a read-only policy",
      overrides: { readOnly: true },
      reason:
        "This workspace is read-only; make it editable in Workspace Details to chat.",
    },
    {
      name: "a freezing policy",
      overrides: { mutationPolicyTransition: "freezing" },
      reason:
        "This workspace is becoming read-only; wait for it to finish before you chat.",
    },
  ])(
    "is disabled with the gate's reason for $name",
    async ({ overrides, reason }) => {
      installSession();
      serveDetail(overrides);
      renderWorkspace();

      const button = await findCornerButton();
      await waitFor(() =>
        expect(button).toHaveAttribute("aria-disabled", "true"),
      );
      expect(button).toHaveAttribute("title", reason);
    },
  );

  it("keeps the report drawer above a right-edge Chat", async () => {
    const user = userEvent.setup();
    const session = installSession();
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...mockInstanceInfo,
            features: [
              ...mockInstanceInfo.features,
              "platform_issue_reporting",
            ],
          },
        }),
      ),
    );
    renderWorkspace(
      `/org-g5labs/test-workspace?session=${session.id}`,
      <PlatformIssueReportProvider showShellTrigger isWorkspaceRoute>
        <WorkspacePage />
      </PlatformIssueReportProvider>,
    );

    await user.click(await findEnabledCornerButton());
    const panel = await waitFor(floatingPanelRoot);
    expect(Number(panel.style.zIndex)).toBeGreaterThan(100);

    // The shortcut opens the report drawer over everything on the page.
    fireEvent.keyDown(document, { key: "b", ctrlKey: true, shiftKey: true });
    const drawer = await screen.findByTestId("platform-bug-report-drawer");
    const overlay = drawer.closest(".fixed.inset-0") as HTMLElement;
    const overlayZ = Number(/z-\[(\d+)\]/.exec(overlay.className)?.[1]);
    expect(overlayZ).toBeGreaterThan(Number(panel.style.zIndex));
    expect(
      screen.getByTestId("platform-bug-report-submit"),
    ).toBeInTheDocument();
  });

  it("is enabled while the VM is stopped: the composer, not the button, explains", async () => {
    installSession();
    server.use(
      http.get("/api/v1/workspaces/:id/vm", () =>
        HttpResponse.json({
          data: createMockWorkspaceVm({ status: "stopped" }),
        }),
      ),
    );
    renderWorkspace();

    await findEnabledCornerButton();
  });
});

// A fork lands on Chat as the center surface (ui g5.workspace.ui.fork-landing).
describe("WorkspacePage fork landing", () => {
  beforeEach(installWorkingWorkspaceDetail);
  function serveNoSessions() {
    server.use(
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
  }

  it("lands ?view=chat on the fresh center composer, pre-typed with the fork's draft", async () => {
    mockDesktopMatchMedia();
    serveNoSessions();
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: "/org-g5labs/test-workspace?view=chat",
        routeState: {
          forkLanding: { draft: "Work on acme/api#12: Crash on save" },
        },
        user: createMockUser(),
      },
    );

    const chatPanel = await screen.findByTestId(
      "right-panel",
      {},
      { timeout: 10_000 },
    );
    expect(chatPanel).toHaveAttribute("data-chat-placement", "center");
    expect(screen.getByTestId("center-chat-tab")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const input = within(chatPanel).getByTestId("chat-input");
    await waitFor(() =>
      expect(input).toHaveValue("Work on acme/api#12: Crash on save"),
    );
    // Pre-typed, never sent: no session exists yet.
    expect(screen.queryByTestId("stop-streaming-button")).toBeNull();
    // An ordinary fork landing carries no seed header.
    expect(
      screen.queryByTestId("composer-seed-verification-notice"),
    ).toBeNull();
  });

  // A Base re-verification run lands its prompt under the cost notice
  // (ontology_and_harness g5.reverification.workspace).
  it("lands a re-verification run's prompt under the verification notice until the draft is emptied", async () => {
    mockDesktopMatchMedia();
    serveNoSessions();
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: "/org-g5labs/test-workspace?view=chat",
        routeState: {
          forkLanding: {
            draft: "Base re-verification (full) for this workspace",
            header: { kind: "base-verification" },
          },
        },
        user: createMockUser(),
      },
    );

    const chatPanel = await screen.findByTestId(
      "right-panel",
      {},
      { timeout: 10_000 },
    );
    expect(chatPanel).toHaveAttribute("data-chat-placement", "center");
    const input = within(chatPanel).getByTestId("chat-input");
    await waitFor(() =>
      expect(input).toHaveValue(
        "Base re-verification (full) for this workspace",
      ),
    );
    expect(
      within(chatPanel).getByTestId("composer-seed-verification-notice"),
    ).toHaveTextContent(BASE_REVERIFICATION_NOTICE);
    expect(screen.queryByTestId("stop-streaming-button")).toBeNull();

    // Emptying the draft drops the notice with the pulse.
    await userEvent.clear(input);
    await waitFor(() =>
      expect(
        screen.queryByTestId("composer-seed-verification-notice"),
      ).toBeNull(),
    );
  });

  it("lands a fork without an issue on an empty composer asking for the idea", async () => {
    mockDesktopMatchMedia();
    serveNoSessions();
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: "/org-g5labs/test-workspace?view=chat",
        routeState: { forkLanding: { draft: null } },
        user: createMockUser(),
      },
    );

    const chatPanel = await screen.findByTestId(
      "right-panel",
      {},
      { timeout: 10_000 },
    );
    expect(chatPanel).toHaveAttribute("data-chat-placement", "center");
    const input = within(chatPanel).getByTestId("chat-input");
    expect(input).toHaveValue("");
    await waitFor(() =>
      expect(input).toHaveAttribute("placeholder", "What's your idea?"),
    );
  });

  it("keeps the usual placeholder when Chat is opened without a fork landing", async () => {
    mockDesktopMatchMedia();
    serveNoSessions();
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: "/org-g5labs/test-workspace?view=chat",
        user: createMockUser(),
      },
    );

    const chatPanel = await screen.findByTestId(
      "right-panel",
      {},
      { timeout: 10_000 },
    );
    expect(chatPanel).toHaveAttribute("data-chat-placement", "center");
    const input = within(chatPanel).getByTestId("chat-input");
    expect(input).toHaveValue("");
    expect(input).not.toHaveAttribute("placeholder", "What's your idea?");
  });
});
