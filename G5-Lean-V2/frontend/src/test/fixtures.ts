// =============================================================================
// Test Fixtures — Mock Data Re-exports & Factory Functions
// =============================================================================
// Re-exports relevant mock data from client/src/data/mock.ts with convenient
// camelCase aliases and provides factory functions for creating test-specific
// data variants.
// =============================================================================

import type {
  ActivityEntryDTO,
  ChatMessageDTO,
  ChatSessionDTO,
  CommentDTO,
  DocNode,
  G5Notification,
  OrgKind,
  TaskDetail,
  WorkspaceActivityDTO,
  WorkspaceSummary,
  WorkspaceVmView,
} from "../lib/types";
import { CLAUDE_CHAT_DEFAULT_MODEL } from "../lib/claudeModels";

// ---------------------------------------------------------------------------
// Re-exports from mock.ts for convenient test imports
// ---------------------------------------------------------------------------

export {
  DEFAULT_SETTINGS,
  DIFF_LINES,
  getMockChatMessages,
  getMockChatSessions,
  getMockComments,
  getMockFileContent,
  getMockFileTree,
  getMockNotifications,
  getMockTask,
  getMockWorkspace,
  getMockWorkspaceSettings,
  MOCK_CHAT_MESSAGES,
  MOCK_CHAT_MODELS,
  MOCK_CHAT_SESSIONS,
  MOCK_COMMENTS,
  MOCK_CURRENT_USER,
  MOCK_DOCS,
  MOCK_FILE_CONTENT,
  MOCK_FILE_TREES,
  MOCK_INSTANCE_HEALTH,
  MOCK_INSTANCE_INFO,
  MOCK_LINE_COMMENTS,
  MOCK_NOTIFICATIONS,
  MOCK_ORG_INVITATIONS,
  MOCK_ORG_MEMBERS,
  MOCK_ORGS,
  MOCK_SPEND_HISTORY,
  MOCK_TASK_TIMELINES,
  MOCK_TASKS,
  MOCK_USER,
  MOCK_WORKSPACE_DETAILS,
  MOCK_WORKSPACE_SETTINGS,
  MOCK_WORKSPACE_SPEND,
  MOCK_WORKSPACES,
  TERMINAL_OUTPUT,
  THEME_OPTIONS,
} from "../data/mock";

// Convenient camelCase aliases
export {
  MOCK_CHAT_MESSAGES as mockChatMessages,
  MOCK_CHAT_MODELS as mockChatModels,
  MOCK_CHAT_SESSIONS as mockChatSessions,
  MOCK_COMMENTS as mockComments,
  MOCK_CURRENT_USER as mockCurrentUser,
  MOCK_DOCS as mockDocs,
  MOCK_FILE_CONTENT as mockFileContent,
  MOCK_FILE_TREES as mockFileTrees,
  MOCK_INSTANCE_HEALTH as mockInstanceHealth,
  MOCK_INSTANCE_INFO as mockInstanceInfo,
  MOCK_ORG_INVITATIONS as mockInvitations,
  MOCK_NOTIFICATIONS as mockNotifications,
  MOCK_ORG_MEMBERS as mockOrgMembers,
  MOCK_ORGS as mockOrgs,
  MOCK_SPEND_HISTORY as mockSpendHistory,
  MOCK_TASKS as mockTasks,
  MOCK_TASK_TIMELINES as mockTimeline,
  MOCK_WORKSPACE_DETAILS as mockWorkspaceDetails,
  MOCK_WORKSPACES as mockWorkspaces,
  MOCK_WORKSPACE_SETTINGS as mockWorkspaceSettings,
  MOCK_WORKSPACE_SPEND as mockWorkspaceSpend,
} from "../data/mock";

// ---------------------------------------------------------------------------
// Mock User Type (for test configuration)
// ---------------------------------------------------------------------------

export interface MockUser {
  id: string;
  email: string;
  displayName: string;
  avatarUrl: string | null;
  /** Currently-active org (mirrors CurrentUserDTO.activeOrgId). Defaults to the
   *  first org so active-org-aware surfaces resolve a realistic value. */
  activeOrgId?: string | null;
  orgs: Array<{
    orgId: string;
    orgName: string;
    orgSlug: string;
    role: "admin" | "member" | "viewer";
    /** Defaults to `tenant` in the mocked `/users/me` response. */
    orgKind?: OrgKind;
  }>;
  scopes: string[];
}

// ---------------------------------------------------------------------------
// JWT Mock Constant
// ---------------------------------------------------------------------------

/**
 * Valid-format JWT string that the client's JWT decoder can parse without errors.
 * Header: {"alg":"HS256","typ":"JWT"}
 * Payload: {"sub":"user-alex","email":"alex@g5labs.dev","org":"org-g5labs","role":"admin","scopes":["workspaces:read","workspaces:write","tasks:execute","workers:admin","org:admin","sessions:write"],"iat":1700000000,"exp":1700003600}
 */
export const MOCK_JWT =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyLWFsZXgiLCJlbWFpbCI6ImFsZXhAZzVsYWJzLmRldiIsInJvbGUiOiJhZG1pbiIsInNjb3BlcyI6WyJ3b3Jrc3BhY2VzOnJlYWQiLCJ3b3Jrc3BhY2VzOndyaXRlIiwiam9iczpleGVjdXRlIiwid29ya2VyczphZG1pbiIsIm9yZzphZG1pbiIsInNlc3Npb25zOndyaXRlIl0sImlhdCI6MTcwMDAwMDAwMCwiZXhwIjoxNzAwMDAzNjAwLCJvcmciOiJvcmctZzVsYWJzIn0.mock-signature";

// ---------------------------------------------------------------------------
// Scope Constants
// ---------------------------------------------------------------------------

/** All scopes — Admin level */
export const MOCK_ADMIN_SCOPES: string[] = [
  "workspaces:read",
  "workspaces:write",
  "tasks:execute",
  "tasks:read",
  "workers:admin",
  "workers:read",
  "org:admin",
  "billing:admin",
  "sessions:write",
  "sessions:read",
];

/** Read-only scopes — viewer level */
export const MOCK_VIEWER_SCOPES: string[] = [
  "workspaces:read",
  "sessions:read",
  "tasks:read",
];

/** Read + write scopes — editor level */
export const MOCK_EDITOR_SCOPES: string[] = [
  "workspaces:read",
  "workspaces:write",
  "sessions:read",
  "sessions:write",
  "tasks:read",
];

// ---------------------------------------------------------------------------
// Factory Functions
// ---------------------------------------------------------------------------

/**
 * Create a mock user with sensible defaults, merging overrides.
 */
export function createMockUser(overrides?: Partial<MockUser>): MockUser {
  return {
    id: "user-alex",
    email: "alex@g5labs.dev",
    displayName: "Alex Chen",
    avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
    activeOrgId: "org-g5labs",
    orgs: [
      {
        orgId: "org-g5labs",
        orgName: "G5 Labs",
        orgSlug: "g5labs",
        role: "admin",
      },
      {
        orgId: "org-acme",
        orgName: "Acme Corp",
        orgSlug: "acme",
        role: "admin",
      },
    ],
    scopes: MOCK_ADMIN_SCOPES,
    ...overrides,
  };
}

/**
 * Create a mock workspace summary with sensible defaults.
 */
export function createMockWorkspace(
  overrides?: Partial<WorkspaceSummary>,
): WorkspaceSummary {
  return {
    id: "test-workspace",
    name: "Test Workspace",
    description: "A test workspace for integration tests",
    isBase: false,
    readOnly: false,
    isReview: false,
    ownerUserId: "user-alex",
    createdBy: null,
    clonedFromId: null,
    branches: [
      {
        workspaceId: "test-workspace",
        repoId: "repo-1",
        repoName: "g5-common",
        branchName: "main",
        trackingMode: "tracking",
        headCommit: "abc123def456789",
        divergence: 0,
        behindBase: 0,
        hasDraft: false,
        dirtyFileCount: 0,
      },
    ],
    tasksCompleted: 15,
    totalTasks: 20,
    runningTime: "2h 15m",
    cost: 12.5,
    concurrentTasks: 3,
    lastUpdated: "2024-11-15T10:00:00Z",
    createdAt: "2024-11-01T08:00:00Z",
    ...overrides,
  };
}

/**
 * Create a mock task detail with sensible defaults.
 */
export function createMockTask(overrides?: Partial<TaskDetail>): TaskDetail {
  return {
    id: "test-task-1",

    name: "Uplift auth/validator",
    taskType: "code",
    status: "running",
    workspaceId: "test-workspace",
    workspaceName: "Test Workspace",

    specContext: null,
    dependsOn: [],
    filesChanged: [],
    costUsd: 0.75,
    sessionId: null,
    assignedAt: "2024-11-15T10:00:00Z",
    completedAt: null,
    ...overrides,
  };
}

/**
 * Create a mock chat session with sensible defaults.
 */
export function createMockChatSession(
  overrides?: Partial<ChatSessionDTO>,
): ChatSessionDTO {
  return {
    id: "test-session-1",
    workspaceId: "test-workspace",
    scopeType: "workspace",
    scopeRef: "test-workspace",
    model: CLAUDE_CHAT_DEFAULT_MODEL,
    agentProvider: "claude_code",
    title: "Test Chat Session",
    summary: "A test chat session for integration tests",
    tags: [],
    messageCount: 5,
    totalInputTokens: 1000,
    totalOutputTokens: 500,
    totalCostUsd: 0.15,
    forkedFromId: null,
    linkedCommits: [],
    createdAt: "2024-11-15T10:00:00Z",
    updatedAt: "2024-11-15T10:05:00Z",
    canSend: true,
    ...overrides,
  };
}

/**
 * Create a mock chat message with sensible defaults.
 */
export function createMockChatMessage(
  overrides?: Partial<ChatMessageDTO>,
): ChatMessageDTO {
  return {
    id: "test-msg-1",
    sessionId: "test-session-1",
    role: "assistant",
    content: "This is a test response from the assistant.",
    artifacts: [],
    citations: [],
    inputTokens: 100,
    outputTokens: 50,
    status: "complete",
    messageIndex: 0,
    createdAt: "2024-11-15T10:05:00Z",
    ...overrides,
  };
}

/**
 * Create a mock comment with sensible defaults.
 */
export function createMockComment(overrides?: Partial<CommentDTO>): CommentDTO {
  return {
    id: "comment-test-1",
    workspaceId: "test-workspace",
    authorId: "user-alex",
    authorName: "Alex Chen",
    authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
    scope: "node",
    repoId: null,
    filePath: null,
    lineNumber: null,
    commitSha: "abc123",
    content: "This is a test comment.",
    resolved: false,
    resolvedAt: null,
    originBranch: "main",
    replyCount: 0,
    createdAt: "2024-11-15T10:10:00Z",
    updatedAt: "2024-11-15T10:10:00Z",
    ...overrides,
  };
}

/**
 * Create a mock notification with sensible defaults.
 */
export function createMockNotification(
  overrides?: Partial<G5Notification>,
): G5Notification {
  return {
    id: "notif-test-1",
    workspaceId: "task-graph-rework",
    severity: "info",
    category: "task_blocked",
    title: "Test Notification",
    message: "This is a test notification for integration tests.",
    taskId: "test-task-1",

    commentId: null,
    replyId: null,
    actions: [],
    createdAt: "2024-11-15T10:15:00Z",
    expiresAt: null,
    readAt: null,
    ...overrides,
  };
}

/**
 * One Activity page (`GET /workspaces/:id/activity`) without a primary issue:
 * workspace context including the workbench entries projected from
 * `workspace_phase_events` and `workspace_pull_request_events`
 * (workbench g5.workbench.landing).
 */
/**
 * Ongoing work (ui g5.ui.activity-current): a running conversation, one
 * waiting for input and a queued command, newest last activity first.
 */
export const MOCK_WORKSPACE_ACTIVITY_CURRENT: ActivityEntryDTO[] = [
  {
    key: "chat:c9",
    at: "2026-01-07T10:00:00Z",
    source: "workspace",
    kind: "chat",
    sessionId: "c9",
    title: "Refactor scheduler",
    messageCount: 14,
    lastActivityAt: "2026-01-07T10:00:00Z",
    taskStatus: "running",
  },
  {
    key: "chat:c8",
    at: "2026-01-07T09:00:00Z",
    source: "workspace",
    kind: "chat",
    sessionId: "c8",
    title: "Write migration",
    messageCount: 6,
    lastActivityAt: "2026-01-07T09:00:00Z",
    taskStatus: "waiting",
  },
  {
    key: "task:t1",
    at: "2026-01-05T10:00:00Z",
    source: "workspace",
    kind: "task",
    taskId: "t1",
    name: "Preview",
    taskKind: "preview",
    status: "queued",
    reason: null,
  },
];

export const MOCK_WORKSPACE_ACTIVITY: WorkspaceActivityDTO = {
  primaryIssue: null,
  issueError: null,
  issueHistoryTruncated: false,
  entries: [
    {
      key: "chat:chat-tgr-1",
      at: "2024-12-10T09:00:00Z",
      source: "workspace",
      kind: "chat",
      sessionId: "chat-tgr-1",
      title: "Implementation chat",
      messageCount: 4,
      lastActivityAt: "2024-12-10T10:00:00Z",
    },
    {
      key: "phase:phase-evt-1",
      at: "2024-12-11T09:00:00Z",
      source: "workspace",
      kind: "phase_changed",
      fromPhase: "be",
      toPhase: "se",
      actor: { userId: "user-alex", displayName: "Alex Chen" },
    },
    {
      key: "pull-request:pr-evt-1",
      at: "2024-12-12T09:00:00Z",
      source: "workspace",
      kind: "pull_request",
      pullRequestId: "pr-1",
      eventType: "created",
      title: "Task graph rework",
      targetWorkspaceName: "base",
      message: null,
      repositoryCount: 1,
      providerRepoFullName: "g5labs/g5",
      providerPrNumber: 41,
      providerPrUrl: "https://github.com/g5labs/g5/pull/41",
      actor: { userId: "user-alex", displayName: "Alex Chen" },
      failureCount: null,
      firstFailedAt: null,
    },
    {
      key: "pull-request:pr-evt-2",
      at: "2024-12-13T09:00:00Z",
      source: "workspace",
      kind: "pull_request",
      pullRequestId: "pr-1",
      eventType: "merge_blocked",
      title: "Task graph rework",
      targetWorkspaceName: "base",
      message: "conflict in docs/plan.md",
      repositoryCount: 1,
      providerRepoFullName: null,
      providerPrNumber: null,
      providerPrUrl: null,
      actor: null,
      failureCount: null,
      firstFailedAt: null,
    },
  ],
  earlierCursor: null,
  composer: null,
};

/** A workspace VM view (`GET /workspaces/:id/vm`) with every field set. */
export function createMockWorkspaceVm(
  overrides: Partial<WorkspaceVmView> = {},
): WorkspaceVmView {
  return {
    orgId: "org",
    workspaceId: "workspace",
    ownerUserId: "owner",
    status: "stopped",
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
    ...overrides,
  };
}

let docNodeSeq = 0;

/**
 * A `DocNode` with sensible defaults: a workspace document at `filePath`
 * (guided-journey evidence, docs panels). The id and name follow the path.
 */
export function createMockDocNode(overrides: Partial<DocNode> = {}): DocNode {
  docNodeSeq += 1;
  const filePath = overrides.filePath ?? `docs/base/doc-${docNodeSeq}.md`;
  return {
    id: overrides.id ?? `doc:${filePath}`,
    name: overrides.name ?? filePath.split("/").pop() ?? filePath,
    docType: "doc",
    filePath,
    repoId: "repo-docs",
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2026-10-01T00:00:00Z",
    createdAt: "2026-10-01T00:00:00Z",
    ...overrides,
  } as DocNode;
}
