// =============================================================================
// G5 Client Mock Data — Development Fallback Data
// =============================================================================
// Comprehensive set of realistic, typed mock data instances covering every
// entity type in the G5 client frontend. Serves as development fallback when
// no backend server is available.
//
// All exports use UPPER_SNAKE_CASE. No default export.
// All cross-entity references are internally consistent.
// =============================================================================

import type {
  BranchSummary,
  ChatMessageDTO,
  ChatSessionDTO,
  CommentDTO,
  CurrentUserDTO,
  DiffLine,
  DocNode,
  FileTreeEntry,
  G5Notification,
  HealthStatus,
  InstanceInfoDTO,
  Organization,
  OrgInvitationDTO,
  OrgMemberDTO,
  SpendBreakdownData,
  SpendHistoryDTO,
  TaskDetail,
  ThemeName,
  TimelineEntry,
  UserProfile,
  UserRef,
  UserSettings,
  WorkspaceDetail,
  WorkspaceSettings,
  WorkspaceSummary,
} from "../lib/types";

// ---------------------------------------------------------------------------
// Shared UserRef instances (reused across mock data)
// ---------------------------------------------------------------------------

const USER_ALEX: UserRef = {
  id: "user-alex",
  displayName: "Alex Chen",
  avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
  initial: "A",
  color: "blue",
};

const USER_JORDAN: UserRef = {
  id: "user-jordan",
  displayName: "Jordan Rivera",
  avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Jordan",
  initial: "J",
  color: "green",
};

const USER_TAYLOR: UserRef = {
  id: "user-taylor",
  displayName: "Taylor Kim",
  avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Taylor",
  initial: "T",
  color: "purple",
};

const USER_SAM: UserRef = {
  id: "user-sam",
  displayName: "Sam Patel",
  avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Sam",
  initial: "S",
  color: "orange",
};

// ---------------------------------------------------------------------------
// 1. User & Auth
// ---------------------------------------------------------------------------

export const MOCK_USER: UserProfile = {
  id: "user-alex",
  name: "Alex Chen",
  email: "alex@g5labs.dev",
  avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
  usage: {
    totalCost: 284.5,
    totalTokens: 12_450_000,
    totalTasks: 847,
  },
};

export const MOCK_CURRENT_USER: CurrentUserDTO = {
  id: "user-alex",
  email: "alex@g5labs.dev",
  displayName: "Alex Chen",
  avatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
  oauthProvider: "github",
  activeOrgId: "org-g5labs",
  orgs: [
    {
      orgId: "org-g5labs",
      orgName: "G5 Labs",
      orgSlug: "g5labs",
      role: "admin",
      orgKind: "tenant",
    },
    {
      orgId: "org-acme",
      orgName: "Acme Corp",
      orgSlug: "acme",
      role: "admin",
      orgKind: "tenant",
    },
    {
      orgId: "org-personal",
      orgName: "Personal",
      orgSlug: "personal",
      role: "admin",
      orgKind: "tenant",
    },
  ],
  scmConnections: [
    {
      provider: "github",
      providerUsername: "alexchen",
      scopes: ["repo", "read:org", "workflow"],
      hasInstallation: true,
      connectedAt: "2024-01-15T10:00:00Z",
    },
    {
      provider: "gitlab",
      providerUsername: "alex.chen",
      scopes: ["api", "read_repository"],
      hasInstallation: false,
      connectedAt: "2024-03-20T14:30:00Z",
    },
  ],
  createdAt: "2024-01-10T08:00:00Z",
  // The mock person has finished the tour, so mock pages stay tour-free;
  // the onboarding preview overrides this (g5.ui.onboarding-tour).
  onboardingStep: 7,
  onboardingSkipped: false,
};

// ---------------------------------------------------------------------------
// 2. Organizations
// ---------------------------------------------------------------------------

export const MOCK_ORGS: Organization[] = [
  {
    id: "org-g5labs",
    name: "G5 Labs",
    plan: "enterprise",
    role: "admin",
    billing: { currentSpend: 1240.0, budget: 5000.0, period: "monthly" },
    usage: {
      inputTokens: 45_000_000,
      outputTokens: 12_000_000,
      tasksRun: 3200,
      agentHours: 156.5,
    },
  },
  {
    id: "org-acme",
    name: "Acme Corp",
    plan: "team",
    role: "admin",
    billing: { currentSpend: 320.0, budget: 1000.0, period: "monthly" },
    usage: {
      inputTokens: 8_500_000,
      outputTokens: 2_100_000,
      tasksRun: 540,
      agentHours: 28.3,
    },
  },
  {
    id: "org-personal",
    name: "Personal",
    plan: "free",
    role: "admin",
    billing: { currentSpend: 12.5, budget: 50.0, period: "monthly" },
    usage: {
      inputTokens: 450_000,
      outputTokens: 120_000,
      tasksRun: 42,
      agentHours: 2.1,
    },
  },
];

// ---------------------------------------------------------------------------
// 3. Org Members & Invitations
// ---------------------------------------------------------------------------

export const MOCK_ORG_MEMBERS: Record<string, OrgMemberDTO[]> = {
  "org-g5labs": [
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-alex",
      email: "alex@g5labs.dev",
      displayName: "Alex Chen",
      avatarUrl: USER_ALEX.avatarUrl,
      role: "admin",
      joinedAt: "2024-01-10T08:00:00Z",
    },
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-jordan",
      email: "jordan@g5labs.dev",
      displayName: "Jordan Rivera",
      avatarUrl: USER_JORDAN.avatarUrl,
      role: "admin",
      joinedAt: "2024-01-12T09:00:00Z",
    },
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-taylor",
      email: "taylor@g5labs.dev",
      displayName: "Taylor Kim",
      avatarUrl: USER_TAYLOR.avatarUrl,
      role: "member",
      joinedAt: "2024-02-01T10:00:00Z",
    },
  ],
  "org-acme": [
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-alex",
      email: "alex@acme.co",
      displayName: "Alex Chen",
      avatarUrl: USER_ALEX.avatarUrl,
      role: "admin",
      joinedAt: "2024-03-01T08:00:00Z",
    },
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-sam",
      email: "sam@acme.co",
      displayName: "Sam Patel",
      avatarUrl: USER_SAM.avatarUrl,
      role: "member",
      joinedAt: "2024-03-05T09:00:00Z",
    },
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-taylor",
      email: "taylor@acme.co",
      displayName: "Taylor Kim",
      avatarUrl: USER_TAYLOR.avatarUrl,
      role: "member",
      joinedAt: "2024-03-10T10:00:00Z",
    },
  ],
  "org-personal": [
    {
      deactivatedAt: null,
      activityRevision: 0,
      shutdownPending: false,
      userId: "user-alex",
      email: "alex@g5labs.dev",
      displayName: "Alex Chen",
      avatarUrl: USER_ALEX.avatarUrl,
      role: "admin",
      joinedAt: "2024-01-10T08:00:00Z",
    },
  ],
};

export const MOCK_ORG_INVITATIONS: Record<string, OrgInvitationDTO[]> = {
  "org-g5labs": [
    {
      id: "inv-1",
      orgId: "org-g5labs",
      email: "newdev@example.com",
      role: "member",
      invitedBy: USER_ALEX,
      status: "pending",
      expiresAt: "2024-12-20T08:00:00Z",
      createdAt: "2024-12-13T08:00:00Z",
    },
    {
      id: "inv-2",
      orgId: "org-g5labs",
      email: "contractor@example.com",
      role: "viewer",
      invitedBy: USER_JORDAN,
      status: "pending",
      expiresAt: "2024-12-22T10:00:00Z",
      createdAt: "2024-12-15T10:00:00Z",
    },
  ],
  "org-acme": [
    {
      id: "inv-3",
      orgId: "org-acme",
      email: "intern@acme.co",
      role: "viewer",
      invitedBy: USER_ALEX,
      status: "pending",
      expiresAt: "2024-12-25T08:00:00Z",
      createdAt: "2024-12-18T08:00:00Z",
    },
    {
      id: "inv-4",
      orgId: "org-acme",
      email: "senior@acme.co",
      role: "admin",
      invitedBy: USER_ALEX,
      status: "pending",
      expiresAt: "2024-12-24T14:00:00Z",
      createdAt: "2024-12-17T14:00:00Z",
    },
  ],
  "org-personal": [],
};

// ---------------------------------------------------------------------------
// 4. Branch definitions (shared across workspaces)
// ---------------------------------------------------------------------------

const BRANCH_TGR_COMMON: BranchSummary = {
  workspaceId: "task-graph-rework",
  repoId: "repo-g5-client",
  repoName: "g5-client",
  branchName: "feat/workspace-ui",
  trackingMode: "tracking",
  headCommit: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
  divergence: 5,
  behindBase: 0,
  hasDraft: true,
  dirtyFileCount: 5,
};

const BRANCH_TGR_API: BranchSummary = {
  workspaceId: "task-graph-rework",
  repoId: "repo-g5-api",
  repoName: "g5-api",
  branchName: "feat/workspace-ui",
  trackingMode: "tracking",
  headCommit: "b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1",
  divergence: 3,
  behindBase: 0,
  hasDraft: true,
  dirtyFileCount: 3,
};

const BRANCH_AUTH_API: BranchSummary = {
  workspaceId: "auth-refactor",
  repoId: "repo-g5d",
  repoName: "g5d",
  branchName: "feat/agent-pipeline",
  trackingMode: "tracking",
  headCommit: "c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2",
  divergence: 4,
  behindBase: 0,
  hasDraft: true,
  dirtyFileCount: 4,
};

const BRANCH_NEW_COMMON: BranchSummary = {
  workspaceId: "new-feature-idea",
  repoId: "repo-g5-common",
  repoName: "g5-common",
  branchName: "feat/new-feature-idea",
  trackingMode: "tracking",
  headCommit: "d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3",
  divergence: 0,
  behindBase: 0,
  hasDraft: false,
  dirtyFileCount: 0,
};

// ---------------------------------------------------------------------------
// 5. Workspaces
// ---------------------------------------------------------------------------

export const MOCK_WORKSPACES: WorkspaceSummary[] = [
  {
    id: "base",
    name: "base",
    description: "Main workspace — the base branch for G5 Labs.",
    isBase: true,
    readOnly: false,
    ownerUserId: "user-alex",
    createdBy: "user-alex",
    clonedFromId: null,
    branches: [
      { ...BRANCH_TGR_COMMON, workspaceId: "base", branchName: "main" },
    ],
    tasksCompleted: 20,
    totalTasks: 24,
    runningTime: "6h 30m",
    cost: 82.4,
    concurrentTasks: 2,
    lastUpdated: "2024-12-16T08:00:00Z",
    createdAt: "2024-11-01T08:00:00Z",
  },
  {
    id: "task-graph-rework",
    name: "Task Graph Rework",
    description:
      "Refactor the task dependency graph to support parallel execution and better error recovery.",
    isBase: false,
    readOnly: false,
    ownerUserId: "user-alex",
    createdBy: "user-alex",
    clonedFromId: "base",
    branches: [BRANCH_TGR_COMMON, BRANCH_TGR_API],
    tasksCompleted: 6,
    totalTasks: 12,
    runningTime: "3h 15m",
    cost: 45.2,
    concurrentTasks: 1,
    lastUpdated: "2024-12-15T14:30:00Z",
    createdAt: "2024-11-10T08:00:00Z",
  },
  {
    id: "auth-refactor",
    name: "Auth Refactor",
    description:
      "Migrate authentication from session-based to JWT with OAuth2 provider support.",
    isBase: false,
    readOnly: false,
    ownerUserId: "user-alex",
    createdBy: "user-jordan",
    clonedFromId: "base",
    branches: [BRANCH_AUTH_API],
    tasksCompleted: 8,
    totalTasks: 8,
    runningTime: "2h 45m",
    cost: 12.8,
    concurrentTasks: 0,
    lastUpdated: "2024-12-14T10:00:00Z",
    createdAt: "2024-10-15T08:00:00Z",
  },
  {
    id: "new-feature-idea",
    name: "New Feature Idea",
    description:
      "Exploring a new approach to incremental compilation with distributed state merging.",
    isBase: false,
    readOnly: false,
    ownerUserId: "user-alex",
    createdBy: "user-taylor",
    clonedFromId: "task-graph-rework",
    branches: [BRANCH_NEW_COMMON],
    tasksCompleted: 0,
    totalTasks: 0,
    runningTime: null,
    cost: 0,
    concurrentTasks: 0,
    lastUpdated: "2024-12-10T16:00:00Z",
    createdAt: "2024-12-10T08:00:00Z",
  },
];

// ---------------------------------------------------------------------------
// 6. Workspace Details
// ---------------------------------------------------------------------------

export const MOCK_WORKSPACE_DETAILS: Record<string, WorkspaceDetail> = {
  base: {
    id: "base",
    orgId: "org-g5labs",
    name: "base",
    isBase: true,
    readOnly: false,
    description: "Main workspace — the base branch for G5 Labs.",
    clonedFromId: null,
    docsRepoId: null,
    createdBy: "user-alex",
    ownerUserId: "user-alex",
    createdAt: "2024-11-01T08:00:00Z",
    updatedAt: "2024-12-16T08:00:00Z",
    pushMode: "manual",
    phase: "be",
    branches: [
      { ...BRANCH_TGR_COMMON, workspaceId: "base", branchName: "main" },
    ],
    docsFolderPath: "docs/base",
  },
  "task-graph-rework": {
    id: "task-graph-rework",
    orgId: "org-g5labs",
    name: "Task Graph Rework",
    isBase: false,
    readOnly: false,
    description:
      "Refactor the task dependency graph to support parallel execution and better error recovery.",
    clonedFromId: "base",
    docsRepoId: null,
    createdBy: "user-alex",
    ownerUserId: "user-alex",
    createdAt: "2024-11-10T08:00:00Z",
    updatedAt: "2024-12-15T14:30:00Z",
    pushMode: "manual",
    phase: "be",
    branches: [BRANCH_TGR_COMMON, BRANCH_TGR_API],
    docsFolderPath: "docs/task-graph-rework",
  },
  "auth-refactor": {
    id: "auth-refactor",
    orgId: "org-g5labs",
    name: "Auth Refactor",
    isBase: false,
    readOnly: false,
    description:
      "Migrate authentication from session-based to JWT with OAuth2 provider support.",
    clonedFromId: "base",
    docsRepoId: null,
    createdBy: "user-jordan",
    ownerUserId: "user-alex",
    createdAt: "2024-10-15T08:00:00Z",
    updatedAt: "2024-12-14T10:00:00Z",
    pushMode: "manual",
    phase: "be",
    branches: [BRANCH_AUTH_API],
    docsFolderPath: "docs/auth-refactor",
  },
  "new-feature-idea": {
    id: "new-feature-idea",
    orgId: "org-g5labs",
    name: "New Feature Idea",
    isBase: false,
    readOnly: false,
    description:
      "Exploring a new approach to incremental compilation with distributed state merging.",
    clonedFromId: "task-graph-rework",
    docsRepoId: null,
    createdBy: "user-taylor",
    ownerUserId: "user-alex",
    createdAt: "2024-12-10T08:00:00Z",
    updatedAt: "2024-12-10T16:00:00Z",
    pushMode: "manual",
    phase: "be",
    branches: [BRANCH_NEW_COMMON],
    docsFolderPath: "docs/new-feature-idea",
  },
};

/**
 * Alex's personal workspace (personal_workspace g5.personal.identity): kind
 * personal, one private repository member, no parent. Not in
 * `MOCK_WORKSPACES`; tests that need it in the list add it.
 */
export const MOCK_PERSONAL_WORKSPACE_DETAIL: WorkspaceDetail = {
  id: "personal-alex",
  orgId: "org-g5labs",
  name: "users/alex",
  isBase: false,
  kind: "personal",
  readOnly: false,
  description: null,
  clonedFromId: null,
  docsRepoId: null,
  createdBy: "user-alex",
  ownerUserId: "user-alex",
  createdAt: "2024-12-01T08:00:00Z",
  updatedAt: "2024-12-16T08:00:00Z",
  pushMode: "manual",
  branches: [
    {
      workspaceId: "personal-alex",
      repoId: "repo-personal-alex",
      repoName: "personal",
      branchName: "users/alex",
      trackingMode: "tracking",
      headCommit: "f00df00df00df00df00df00df00df00df00df00d",
      divergence: null,
      behindBase: null,
      hasDraft: false,
      dirtyFileCount: 0,
    },
  ],
  docsFolderPath: "docs/personal-alex",
};
MOCK_WORKSPACE_DETAILS[MOCK_PERSONAL_WORKSPACE_DETAIL.id] =
  MOCK_PERSONAL_WORKSPACE_DETAIL;

// ---------------------------------------------------------------------------
// 10. Tasks
// ---------------------------------------------------------------------------

export const MOCK_TASKS: Record<string, TaskDetail> = {
  // Completed workspace tasks
  "task-tgr-1": {
    id: "task-tgr-1",

    name: "Parse workspace page spec",
    taskType: "code",
    status: "succeeded",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: [],
    filesChanged: ["src/pages/WorkspacePage.tsx", "src/App.tsx"],
    costUsd: 2.1,
    sessionId: null,
    assignedAt: "2024-12-14T10:00:00Z",
    completedAt: "2024-12-14T10:05:00Z",
  },
  "task-tgr-2": {
    id: "task-tgr-2",

    name: "Analyze data layer types",
    taskType: "code",
    status: "succeeded",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-1"],
    filesChanged: ["src/lib/types.ts", "src/lib/api.ts"],
    costUsd: 2.3,
    sessionId: null,
    assignedAt: "2024-12-14T10:10:00Z",
    completedAt: "2024-12-14T10:15:00Z",
  },
  "task-tgr-3": {
    id: "task-tgr-3",

    name: "Generate design tokens",
    taskType: "code",
    status: "succeeded",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-1"],
    filesChanged: ["src/index.css"],
    costUsd: 1.8,
    sessionId: null,
    assignedAt: "2024-12-14T10:10:00Z",
    completedAt: "2024-12-14T10:20:00Z",
  },
  "task-tgr-4": {
    id: "task-tgr-4",

    name: "Build shared UI components",
    taskType: "code",
    status: "succeeded",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-2", "task-tgr-3"],
    filesChanged: [
      "src/components/ui/UI.tsx",
      "src/components/ui/Button.tsx",
      "src/components/ui/Modal.tsx",
    ],
    costUsd: 2.2,
    sessionId: null,
    assignedAt: "2024-12-14T10:20:00Z",
    completedAt: "2024-12-14T11:00:00Z",
  },
  // Workspace tasks in mixed states
  "task-tgr-5": {
    id: "task-tgr-5",

    name: "Implement workspace shell",
    taskType: "code",
    status: "running",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: [],
    filesChanged: ["src/pages/WorkspacePage.tsx", "src/index.css"],
    costUsd: 8.5,
    sessionId: "chat-tgr-2",
    assignedAt: "2024-12-15T08:00:00Z",
    completedAt: null,
  },
  "task-tgr-6": {
    id: "task-tgr-6",

    name: "Uplift left sidebar",
    taskType: "code",
    status: "succeeded",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: [],
    filesChanged: ["src/components/tree/TreeNode.tsx"],
    costUsd: 5.2,
    sessionId: null,
    assignedAt: "2024-12-15T08:02:00Z",
    completedAt: "2024-12-15T08:20:00Z",
  },
  "task-tgr-7": {
    id: "task-tgr-7",

    name: "Uplift top bar",
    taskType: "code",
    status: "succeeded",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: [],
    filesChanged: [
      "src/components/workspace/TopBar.tsx",
      "src/components/settings/UserSettingsPanel.tsx",
    ],
    costUsd: 4.1,
    sessionId: null,
    assignedAt: "2024-12-15T09:00:00Z",
    completedAt: "2024-12-15T09:30:00Z",
  },
  "task-tgr-8": {
    id: "task-tgr-8",

    name: "Build center panel",
    taskType: "code",
    status: "queued",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-5"],
    filesChanged: [],
    costUsd: 0,
    sessionId: null,
    assignedAt: null,
    completedAt: null,
  },
  "task-tgr-9": {
    id: "task-tgr-9",

    name: "Uplift settings panel",
    taskType: "code",
    status: "queued",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-5", "task-tgr-7"],
    filesChanged: [],
    costUsd: 0,
    sessionId: null,
    assignedAt: null,
    completedAt: null,
  },
  // Queued workspace tasks
  "task-tgr-10": {
    id: "task-tgr-10",

    name: "Downlift ontology routes",
    taskType: "code",
    status: "queued",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: [],
    filesChanged: [],
    costUsd: 0,
    sessionId: null,
    assignedAt: null,
    completedAt: null,
  },
  "task-tgr-11": {
    id: "task-tgr-11",

    name: "Downlift workspace routes",
    taskType: "code",
    status: "queued",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-10"],
    filesChanged: [],
    costUsd: 0,
    sessionId: null,
    assignedAt: null,
    completedAt: null,
  },
  "task-tgr-12": {
    id: "task-tgr-12",

    name: "Downlift workspace service",
    taskType: "code",
    status: "queued",
    workspaceId: "task-graph-rework",
    workspaceName: "Task Graph Rework",

    specContext: null,
    dependsOn: ["task-tgr-10"],
    filesChanged: [],
    costUsd: 0,
    sessionId: null,
    assignedAt: null,
    completedAt: null,
  },
};

// ---------------------------------------------------------------------------
// 11. Task Timelines
// ---------------------------------------------------------------------------

export const MOCK_TASK_TIMELINES: Record<string, TimelineEntry[]> = {
  "task-tgr-1": [
    {
      id: "tl-1-1",
      entryType: "thought",
      content:
        "Analyzing workspace page spec structure from .semlift/specs/client/workspace-page...",
      metadata: {},
      createdAt: "2024-12-14T10:01:00Z",
    },
    {
      id: "tl-1-2",
      entryType: "tool_call",
      content: 'read_file("src/pages/WorkspacePage.tsx")',
      metadata: {
        tool: "read_file",
        args: { path: "src/pages/WorkspacePage.tsx" },
      },
      createdAt: "2024-12-14T10:01:30Z",
    },
    {
      id: "tl-1-3",
      entryType: "tool_result",
      content: "File contents: 1286 lines, shell layout with 5 panels",
      metadata: { lines: 1286 },
      createdAt: "2024-12-14T10:01:35Z",
    },
    {
      id: "tl-1-4",
      entryType: "output",
      content:
        "Generated workspace page spec: shell + left-sidebar + top-bar + center-panel + right-panel + bottom-drawer.",
      metadata: {},
      createdAt: "2024-12-14T10:05:00Z",
    },
  ],
  "task-tgr-2": [
    {
      id: "tl-2-1",
      entryType: "thought",
      content: "Building type dependency graph from types.ts exports...",
      metadata: {},
      createdAt: "2024-12-14T10:10:00Z",
    },
    {
      id: "tl-2-2",
      entryType: "tool_call",
      content: 'read_file("src/lib/types.ts")',
      metadata: { tool: "read_file", args: { path: "src/lib/types.ts" } },
      createdAt: "2024-12-14T10:10:30Z",
    },
    {
      id: "tl-2-3",
      entryType: "tool_result",
      content: "Types module: 951 lines, 48 interfaces and type aliases",
      metadata: { lines: 951 },
      createdAt: "2024-12-14T10:10:35Z",
    },
    {
      id: "tl-2-4",
      entryType: "output",
      content:
        "Identified 48 type definitions across 6 categories: DTOs, settings, UI state, chat, tasks, ontology.",
      metadata: {},
      createdAt: "2024-12-14T10:15:00Z",
    },
  ],
  "task-tgr-5": [
    {
      id: "tl-5-1",
      entryType: "thought",
      content:
        "Designing the stacked sidebar layout replacing tabbed navigation...",
      metadata: {},
      createdAt: "2024-12-15T08:01:00Z",
    },
    {
      id: "tl-5-2",
      entryType: "tool_call",
      content: 'write_file("src/pages/WorkspacePage.tsx", ...)',
      metadata: {
        tool: "write_file",
        args: { path: "src/pages/WorkspacePage.tsx" },
      },
      createdAt: "2024-12-15T08:05:00Z",
    },
    {
      id: "tl-5-3",
      entryType: "tool_result",
      content: "File written: 1286 lines",
      metadata: { lines: 1286 },
      createdAt: "2024-12-15T08:05:05Z",
    },
    {
      id: "tl-5-4",
      entryType: "tool_call",
      content: 'run_test("npx tsc --noEmit")',
      metadata: { tool: "run_test", args: { command: "npx tsc --noEmit" } },
      createdAt: "2024-12-15T08:10:00Z",
    },
    {
      id: "tl-5-5",
      entryType: "tool_result",
      content: "TypeScript: No errors found",
      metadata: { passed: 1, failed: 0 },
      createdAt: "2024-12-15T08:10:45Z",
    },
    {
      id: "tl-5-6",
      entryType: "thought",
      content: "Implementing resizable panel layout with drag handles...",
      metadata: {},
      createdAt: "2024-12-15T08:15:00Z",
    },
  ],
  "task-tgr-6": [
    {
      id: "tl-6-1",
      entryType: "thought",
      content:
        "Analyzing TreeNode.tsx stacked sections: Docs, Ontology, Tasks, Files...",
      metadata: {},
      createdAt: "2024-12-15T08:02:00Z",
    },
    {
      id: "tl-6-2",
      entryType: "output",
      content:
        "Left sidebar uplift: 13 lucide-react icons, 4 collapsible sections, lazy child loading.",
      metadata: {},
      createdAt: "2024-12-15T08:20:00Z",
    },
  ],
  "task-tgr-7": [
    {
      id: "tl-7-1",
      entryType: "thought",
      content:
        "Reviewing TopBar layout: branch selector, model badge, settings gear, cost display...",
      metadata: {},
      createdAt: "2024-12-15T09:00:00Z",
    },
    {
      id: "tl-7-2",
      entryType: "output",
      content:
        "Top bar uplift complete: settings gear icon opens UserSettingsPanel flyout.",
      metadata: {},
      createdAt: "2024-12-15T09:30:00Z",
    },
  ],
};

// ---------------------------------------------------------------------------
// 13. Chat Sessions
// ---------------------------------------------------------------------------

export const MOCK_CHAT_SESSIONS: Record<string, ChatSessionDTO[]> = {
  "task-graph-rework": [
    {
      id: "chat-tgr-1",
      workspaceId: "task-graph-rework",
      scopeType: "workspace",
      scopeRef: "task-graph-rework",
      model: "claude-sonnet-4-20250514",
      agentProvider: "claude_code",
      title: "Architecture discussion",
      summary:
        "Discussing the DAG scheduler design and error recovery strategy.",
      tags: [],
      messageCount: 12,
      totalInputTokens: 1200,
      totalOutputTokens: 2400,
      totalCostUsd: 1.2,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2024-12-15T10:00:00Z",
      updatedAt: "2024-12-15T10:30:00Z",
      canSend: true,
    },
    {
      id: "chat-tgr-2",
      workspaceId: "task-graph-rework",
      scopeType: "workspace",
      scopeRef: "task-graph-rework",
      model: "claude-sonnet-4-20250514",
      agentProvider: "claude_code",
      title: "Scheduler implementation",
      summary: "Agent implementing the parallel DAG scheduler.",
      tags: [],
      messageCount: 24,
      totalInputTokens: 4800,
      totalOutputTokens: 9600,
      totalCostUsd: 4.8,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2024-12-15T08:00:00Z",
      updatedAt: "2024-12-15T12:00:00Z",
      canSend: true,
    },
    {
      id: "chat-tgr-3",
      workspaceId: "task-graph-rework",
      scopeType: "node",
      scopeRef: "node-wp-shell",
      model: "claude-sonnet-4-20250514",
      agentProvider: "claude_code",
      title: "Workspace shell review",
      summary: "Reviewing the scheduler implementation for correctness.",
      tags: [],
      messageCount: 8,
      totalInputTokens: 800,
      totalOutputTokens: 1600,
      totalCostUsd: 0.9,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2024-12-15T12:00:00Z",
      updatedAt: "2024-12-15T12:30:00Z",
      canSend: true,
    },
  ],
  "auth-refactor": [
    {
      id: "chat-auth-1",
      workspaceId: "auth-refactor",
      scopeType: "workspace",
      scopeRef: "auth-refactor",
      model: "claude-sonnet-4-20250514",
      agentProvider: "claude_code",
      title: "JWT migration plan",
      summary:
        "Planning the migration from session-based to JWT authentication.",
      tags: [],
      messageCount: 16,
      totalInputTokens: 1600,
      totalOutputTokens: 3200,
      totalCostUsd: 1.6,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2024-12-12T09:00:00Z",
      updatedAt: "2024-12-12T10:00:00Z",
      canSend: true,
    },
  ],
  "new-feature-idea": [
    {
      id: "chat-new-1",
      workspaceId: "new-feature-idea",
      scopeType: "workspace",
      scopeRef: "new-feature-idea",
      model: "claude-sonnet-4-20250514",
      agentProvider: "claude_code",
      title: "live workspace file brainstorm",
      summary:
        "Exploring distributed state merging for incremental compilation.",
      tags: [],
      messageCount: 6,
      totalInputTokens: 600,
      totalOutputTokens: 1200,
      totalCostUsd: 0.45,
      forkedFromId: null,
      linkedCommits: [],
      createdAt: "2024-12-10T16:00:00Z",
      updatedAt: "2024-12-10T16:30:00Z",
      canSend: true,
    },
  ],
};

// ---------------------------------------------------------------------------
// 14. Chat Messages
// ---------------------------------------------------------------------------

export const MOCK_CHAT_MESSAGES: Record<string, ChatMessageDTO[]> = {
  "chat-tgr-1": [
    {
      id: "msg-tgr-1-1",
      sessionId: "chat-tgr-1",
      role: "user",
      content:
        "What's the best approach for the DAG scheduler? Should we use topological sort or a priority queue?",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 0,
      createdAt: "2024-12-15T10:00:00Z",
    },
    {
      id: "msg-tgr-1-2",
      sessionId: "chat-tgr-1",
      role: "assistant",
      content:
        "I recommend a hybrid approach: topological sort for dependency ordering, combined with a priority queue for selecting among ready tasks. This gives us both correctness (respecting dependencies) and flexibility (prioritizing critical path tasks).\n\nHere's a sketch of the core algorithm:",
      artifacts: [
        {
          type: "code",
          title: "DAG Scheduler Core",
          language: "rust",
          content:
            "pub struct DagScheduler {\n    ready_queue: BinaryHeap<PrioritizedTask>,\n    completed: HashSet<TaskId>,\n    blocked: HashMap<TaskId, Vec<TaskId>>,\n}\n\nimpl DagScheduler {\n    pub fn next_task(&mut self) -> Option<Task> {\n        self.ready_queue.pop().map(|pt| pt.task)\n    }\n\n    pub fn complete_task(&mut self, id: TaskId) {\n        self.completed.insert(id);\n        self.unblock_dependents(id);\n    }\n}",
        },
      ],
      citations: [
        {
          label: "WorkspacePage.tsx",
          lineRange: [1, 65],
        },
      ],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 1,
      createdAt: "2024-12-15T10:00:30Z",
    },
    {
      id: "msg-tgr-1-3",
      sessionId: "chat-tgr-1",
      role: "user",
      content:
        "How should we handle task failures? Retry with backoff or cancel its dependants?",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 2,
      createdAt: "2024-12-15T10:05:00Z",
    },
    {
      id: "msg-tgr-1-4",
      sessionId: "chat-tgr-1",
      role: "assistant",
      content:
        "I suggest a configurable policy per VM:\n\n1. **Retry with exponential backoff** (default): Up to 3 retries with 1s, 2s, 4s delays\n2. **Fail fast**: Cancel all pending tasks immediately\n3. **Continue**: Mark failed task, continue with independent branches\n\nThe policy should be set when the task starts.",
      artifacts: [
        {
          type: "table",
          title: "Failure Policy Comparison",
          content:
            "| Policy | Latency Impact | Cost Impact | Use Case |\n|--------|---------------|-------------|----------|\n| Retry | Medium | Medium | Transient errors |\n| Fail Fast | Low | Low | Critical tasks |\n| Continue | High | High | Independent subtasks |",
        },
      ],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 3,
      createdAt: "2024-12-15T10:05:30Z",
    },
  ],
  "chat-tgr-2": [
    {
      id: "msg-tgr-2-1",
      sessionId: "chat-tgr-2",
      role: "system",
      content:
        "Task: Implement workspace shell. Working on node-wp-shell in branch feat/workspace-ui.",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 0,
      createdAt: "2024-12-15T08:00:00Z",
    },
    {
      id: "msg-tgr-2-2",
      sessionId: "chat-tgr-2",
      role: "assistant",
      content:
        "I'll start by reading the existing scheduler module to understand the current implementation.",
      artifacts: [],
      citations: [{ label: "WorkspacePage.tsx" }],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 1,
      createdAt: "2024-12-15T08:00:30Z",
    },
    {
      id: "msg-tgr-2-3",
      sessionId: "chat-tgr-2",
      role: "tool",
      content: "File read: src/scheduler.rs (245 lines)",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 2,
      createdAt: "2024-12-15T08:00:35Z",
    },
    {
      id: "msg-tgr-2-4",
      sessionId: "chat-tgr-2",
      role: "assistant",
      content:
        "The current scheduler uses a simple FIFO queue. I'll refactor it to use a work-stealing design with a priority-based ready queue.",
      artifacts: [
        {
          type: "diff",
          title: "scheduler.rs refactor",
          content:
            "--- a/src/scheduler.rs\n+++ b/src/scheduler.rs\n@@ -1,10 +1,15 @@\n-use std::collections::VecDeque;\n+use std::collections::{BinaryHeap, HashMap, HashSet};\n \n-pub struct Scheduler {\n-    queue: VecDeque<Task>,\n+pub struct DagScheduler {\n+    ready_queue: BinaryHeap<PrioritizedTask>,\n+    completed: HashSet<TaskId>,\n+    blocked: HashMap<TaskId, Vec<TaskId>>,\n+    in_flight: HashSet<TaskId>,\n }",
        },
      ],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "streaming",
      messageIndex: 3,
      createdAt: "2024-12-15T08:01:00Z",
    },
  ],
  "chat-tgr-3": [
    {
      id: "msg-tgr-3-1",
      sessionId: "chat-tgr-3",
      role: "user",
      content:
        "Can you review the scheduler implementation for potential race conditions?",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 0,
      createdAt: "2024-12-15T12:00:00Z",
    },
    {
      id: "msg-tgr-3-2",
      sessionId: "chat-tgr-3",
      role: "assistant",
      content:
        "I found a potential issue in the `complete_task` method. The `unblock_dependents` call should be atomic with the `completed.insert` to prevent a race where a dependent task is scheduled before the completion is recorded.\n\nHere's the fix:",
      artifacts: [
        {
          type: "code",
          title: "Race condition fix",
          language: "rust",
          content:
            "pub fn complete_task(&mut self, id: TaskId) {\n    // Must be atomic: insert completion + unblock\n    let _lock = self.state_lock.write();\n    self.completed.insert(id);\n    self.in_flight.remove(&id);\n    self.unblock_dependents(id);\n}",
        },
      ],
      citations: [
        {
          filePath: "src/pages/WorkspacePage.tsx",
          lineRange: [45, 52],
          label: "shell layout",
        },
      ],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 1,
      createdAt: "2024-12-15T12:00:45Z",
    },
  ],
  "chat-auth-1": [
    {
      id: "msg-auth-1-1",
      sessionId: "chat-auth-1",
      role: "user",
      content: "What's the plan for migrating from session cookies to JWT?",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 0,
      createdAt: "2024-12-12T09:00:00Z",
    },
    {
      id: "msg-auth-1-2",
      sessionId: "chat-auth-1",
      role: "assistant",
      content:
        "Here's the migration plan:\n\n1. Add JWT signing/verification alongside existing session auth\n2. Update OAuth callback to issue JWT instead of session cookie\n3. Add refresh token rotation\n4. Deprecate session endpoints\n5. Remove session middleware\n\nThe JWT will use RS256 with a 15-minute access token and 7-day refresh token.",
      artifacts: [
        {
          type: "diagram",
          title: "JWT Auth Flow",
          content:
            "sequenceDiagram\n    participant C as Client\n    participant A as API\n    participant O as OAuth Provider\n    C->>A: GET /auth/login\n    A->>O: Redirect to OAuth\n    O->>A: Callback with code\n    A->>C: JWT access + refresh tokens",
        },
      ],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 1,
      createdAt: "2024-12-12T09:01:00Z",
    },
  ],
  "chat-new-1": [
    {
      id: "msg-new-1-1",
      sessionId: "chat-new-1",
      role: "user",
      content:
        "How could distributed documents help with incremental compilation state?",
      artifacts: [],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 0,
      createdAt: "2024-12-10T16:00:00Z",
    },
    {
      id: "msg-new-1-2",
      sessionId: "chat-new-1",
      role: "assistant",
      content:
        "distributed documents (Conflict-free Replicated Data Types) could enable multiple workers to independently update compilation state without coordination. Each worker maintains a local replica of the compilation graph, and changes merge automatically.\n\nKey benefits:\n- No central coordinator bottleneck\n- Automatic conflict resolution\n- Offline-capable workers\n\nThe main challenge is designing a live workspace file that captures compilation dependencies efficiently.",
      artifacts: [
        {
          type: "math",
          title: "live workspace file Merge Semantics",
          content: "S_1 \\sqcup S_2 = \\text{LUB}(S_1, S_2)",
        },
      ],
      citations: [],
      inputTokens: null,
      outputTokens: null,
      status: "complete",
      messageIndex: 1,
      createdAt: "2024-12-10T16:01:00Z",
    },
  ],
};

// ---------------------------------------------------------------------------
// 15. Comments
// ---------------------------------------------------------------------------

export const MOCK_COMMENTS: Record<string, CommentDTO[]> = {
  "node-wp-shell": [
    {
      id: "comment-1",
      workspaceId: "task-graph-rework",
      authorId: "user-alex",
      authorName: "Alex Chen",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
      scope: "node",
      repoId: null,
      filePath: null,
      lineNumber: 3,
      commitSha: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
      content:
        "The work-stealing queue approach looks good. Should we add a configurable max queue depth?",
      resolved: false,
      resolvedAt: null,
      originBranch: "feat/task-graph-rework",
      replyCount: 2,
      createdAt: "2024-12-15T11:00:00Z",
      updatedAt: "2024-12-15T11:00:00Z",
    },
    {
      id: "comment-2",
      workspaceId: "task-graph-rework",
      authorId: "user-jordan",
      authorName: "Jordan Rivera",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Jordan",
      scope: "node",
      repoId: null,
      filePath: null,
      lineNumber: 5,
      commitSha: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
      content:
        "Yes, I'd recommend a default of 64 with an env var override. We hit issues with unbounded queues in the old scheduler.",
      resolved: false,
      resolvedAt: null,
      originBranch: "feat/task-graph-rework",
      replyCount: 0,
      createdAt: "2024-12-15T11:15:00Z",
      updatedAt: "2024-12-15T11:15:00Z",
    },
  ],
  "node-wp-left-sidebar": [
    {
      id: "comment-3",
      workspaceId: "task-graph-rework",
      authorId: "user-taylor",
      authorName: "Taylor Kim",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Taylor",
      scope: "node",
      repoId: null,
      filePath: null,
      lineNumber: 4,
      commitSha: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
      content:
        "The cycle detection algorithm should use Tarjan's SCC instead of simple DFS for better performance on large graphs.",
      resolved: true,
      resolvedAt: "2024-12-15T12:00:00Z",
      originBranch: "feat/task-graph-rework",
      replyCount: 1,
      createdAt: "2024-12-14T15:00:00Z",
      updatedAt: "2024-12-15T12:00:00Z",
    },
  ],
  "node-auth-jwt": [
    {
      id: "comment-4",
      workspaceId: "auth-refactor",
      authorId: "user-jordan",
      authorName: "Jordan Rivera",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Jordan",
      scope: "node",
      repoId: null,
      filePath: null,
      lineNumber: 3,
      commitSha: "c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2",
      content:
        "JWT validation should check both exp and nbf claims. Also need to handle clock skew with a 30-second tolerance.",
      resolved: true,
      resolvedAt: "2024-12-13T16:00:00Z",
      originBranch: "feat/auth-refactor",
      replyCount: 3,
      createdAt: "2024-12-13T14:00:00Z",
      updatedAt: "2024-12-13T16:00:00Z",
    },
  ],
};

// ---------------------------------------------------------------------------
// 16. Line Comments
// ---------------------------------------------------------------------------

export const MOCK_LINE_COMMENTS: Record<string, CommentDTO[]> = {
  "src/scheduler.rs:feat/task-graph-rework": [
    {
      id: "lc-1",
      workspaceId: "task-graph-rework",
      authorId: "user-jordan",
      authorName: "Jordan Rivera",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Jordan",
      scope: "line",
      repoId: "repo-g5-code",
      filePath: "src/scheduler.rs",
      lineNumber: 42,
      commitSha: "b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1",
      content:
        "This unwrap() could panic if the task map is empty. Use unwrap_or_default() instead.",
      resolved: false,
      resolvedAt: null,
      originBranch: "feat/task-graph-rework",
      replyCount: 1,
      createdAt: "2024-12-15T13:00:00Z",
      updatedAt: "2024-12-15T13:00:00Z",
    },
  ],
  "src/dag.rs:feat/task-graph-rework": [
    {
      id: "lc-2",
      workspaceId: "task-graph-rework",
      authorId: "user-alex",
      authorName: "Alex Chen",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Alex",
      scope: "line",
      repoId: "repo-g5-code",
      filePath: "src/dag.rs",
      lineNumber: 78,
      commitSha: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
      content:
        "Consider using `petgraph` crate instead of hand-rolling the graph traversal.",
      resolved: true,
      resolvedAt: "2024-12-14T16:00:00Z",
      originBranch: "feat/task-graph-rework",
      replyCount: 2,
      createdAt: "2024-12-14T14:30:00Z",
      updatedAt: "2024-12-14T16:00:00Z",
    },
  ],
  "src/auth/jwt.rs:feat/auth-refactor": [
    {
      id: "lc-3",
      workspaceId: "auth-refactor",
      authorId: "user-sam",
      authorName: "Sam Patel",
      authorAvatarUrl: "https://api.dicebear.com/7.x/avataaars/svg?seed=Sam",
      scope: "line",
      repoId: "repo-g5-code",
      filePath: "src/auth/jwt.rs",
      lineNumber: 15,
      commitSha: "c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2",
      content:
        "The signing key should be loaded from environment, not hardcoded.",
      resolved: true,
      resolvedAt: "2024-12-13T15:00:00Z",
      originBranch: "feat/auth-refactor",
      replyCount: 0,
      createdAt: "2024-12-13T12:00:00Z",
      updatedAt: "2024-12-13T15:00:00Z",
    },
  ],
};

// ---------------------------------------------------------------------------
// 17. Notifications
// ---------------------------------------------------------------------------

export const MOCK_NOTIFICATIONS: Record<string, G5Notification[]> = {
  "task-graph-rework": [
    {
      id: "notif-1",
      workspaceId: "task-graph-rework",
      severity: "info",
      category: "task_blocked",
      title: "Task completed",
      message: 'Task "Compile workspace specs" completed successfully.',
      taskId: null,

      commentId: null,
      replyId: null,
      actions: [
        {
          label: "View Task",
          action: "navigate",
          target: "/g5labs/task-graph-rework/task/task-tgr-1",
        },
      ],
      createdAt: "2024-12-14T11:00:00Z",
      expiresAt: null,
      readAt: null,
    },
    {
      id: "notif-2",
      workspaceId: "task-graph-rework",
      severity: "warning",
      category: "task_blocked",
      title: "Agent needs input on scheduler",
      message:
        'The agent working on task "Implement DAG scheduler" needs clarification on the retry policy.',
      taskId: "task-tgr-5",

      commentId: null,
      replyId: null,
      actions: [
        { label: "Open Chat", action: "open_chat", target: "chat-tgr-2" },
        {
          label: "View Task",
          action: "navigate",
          target: "/workspaces/task-graph-rework/tasks/task-tgr-5",
        },
      ],
      createdAt: "2024-12-15T12:00:00Z",
      expiresAt: null,
      readAt: null,
    },
    {
      id: "notif-3",
      workspaceId: "task-graph-rework",
      severity: "info",
      category: "mention",
      title: "@jordan mentioned you in a comment",
      message:
        'Jordan Rivera mentioned you in a comment on workspace shell: "The stacked sidebar approach..."',
      taskId: null,

      commentId: "comment-2",
      replyId: null,
      actions: [
        {
          label: "View Comment",
          action: "navigate",
          target: "/workspaces/task-graph-rework/nodes/node-wp-shell#comment-2",
        },
      ],
      createdAt: "2024-12-15T11:15:00Z",
      expiresAt: null,
      readAt: null,
    },
    {
      id: "notif-4",
      workspaceId: "task-graph-rework",
      severity: "info",
      category: "comment",
      title: "New reply on sidebar",
      message:
        "New reply on your comment about sidebar sections on workspace shell.",
      taskId: null,

      commentId: "comment-1",
      replyId: "reply-1",
      actions: [
        {
          label: "View Thread",
          action: "navigate",
          target: "/workspaces/task-graph-rework/nodes/node-wp-shell#comment-1",
        },
      ],
      createdAt: "2024-12-15T11:20:00Z",
      expiresAt: null,
      readAt: null,
    },
    {
      id: "notif-5",
      workspaceId: "task-graph-rework",
      severity: "warning",
      category: "spend_warning",
      title: "Workspace approaching spend limit",
      message:
        'Workspace "Task Graph Rework" has spent $45.20 of $100.00 daily budget (45%).',
      taskId: null,

      commentId: null,
      replyId: null,
      actions: [
        {
          label: "View Settings",
          action: "navigate",
          target: "/workspaces/task-graph-rework/settings",
        },
      ],
      createdAt: "2024-12-15T14:00:00Z",
      expiresAt: "2024-12-16T00:00:00Z",
      readAt: null,
    },
    {
      id: "notif-6",
      workspaceId: "task-graph-rework",
      severity: "info",
      category: "agent_error",
      title: "Workspace host gamma provisioning complete",
      message:
        "Worker gamma is now online with 8 CPU cores and 8192 MB memory.",
      taskId: null,

      commentId: null,
      replyId: null,
      actions: [
        {
          label: "View workspace hosts",
          action: "navigate",
          target: "/acme/settings/workers",
        },
      ],
      createdAt: "2024-12-15T14:30:00Z",
      expiresAt: null,
      readAt: null,
    },
  ],
  "auth-refactor": [],
  "new-feature-idea": [],
};

// ---------------------------------------------------------------------------
// 20. Node Diffs
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 20b. Project Docs
// ---------------------------------------------------------------------------

export const MOCK_DOCS: DocNode[] = [
  {
    id: "doc-frontend-design",
    name: "Frontend Design",
    docType: "design",
    filePath: null,
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2024-12-15T10:00:00Z",
    createdAt: "2024-12-15T10:00:00Z",
    updatedAt: "2024-12-15T10:00:00Z",
    title: "Frontend Design",
    category: "design",
    content:
      "# Frontend Design\n\nThe G5 frontend is a React 19 application built with Vite and Tailwind CSS v4. It uses a workspace-centric layout with left sidebar (ontology tree), center editor panel, right chat panel, and bottom execution drawer.",
    preview: "React 19 + Vite + Tailwind CSS v4 workspace layout",
    folder: "Design",
    pinned: true,
  },
  {
    id: "doc-api-spec",
    name: "API & Storage Specification",
    docType: "api",
    filePath: null,
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2024-12-14T08:30:00Z",
    createdAt: "2024-12-14T08:30:00Z",
    updatedAt: "2024-12-14T08:30:00Z",
    title: "API & Storage Specification",
    category: "api",
    content:
      "# API & Storage\n\nAll API endpoints follow REST conventions under `/api/v1/`. Authentication uses JWT bearer tokens. Workspace data is stored in PostgreSQL with JSONB columns for flexible ontology storage.",
    preview: "REST API v1 with JWT auth and PostgreSQL storage",
    folder: "Specifications",
  },
  {
    id: "doc-agent-arch",
    name: "Agent Architecture",
    docType: "architecture",
    filePath: null,
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2024-12-13T14:00:00Z",
    createdAt: "2024-12-13T14:00:00Z",
    updatedAt: "2024-12-13T14:00:00Z",
    title: "Agent Architecture",
    category: "architecture",
    content:
      "# Agent Architecture\n\nThe g5d daemon orchestrates LLM-powered code agents. Each agent runs in a sandboxed Docker container with access to workspace files, git operations, and tool-calling APIs. Agents follow a plan→execute→verify loop.",
    preview: "LLM-powered agents in sandboxed Docker containers",
    folder: "Architecture",
  },
  {
    id: "doc-ontology-guide",
    name: "Ontology Structure Guide",
    docType: "guide",
    filePath: null,
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2024-12-12T16:00:00Z",
    createdAt: "2024-12-12T16:00:00Z",
    updatedAt: "2024-12-12T16:00:00Z",
    title: "Ontology Structure Guide",
    category: "guide",
    content:
      "# Ontology Guide\n\nEach workspace branch has an ontology tree that maps high-level concepts (pages, components, services) to source files. Nodes can be uplifted (spec→code) or downlifted (code→spec). The tree is browsable in the left sidebar.",
    preview: "Ontology tree mapping concepts to source files",
    folder: "Guides",
  },
  {
    id: "doc-worker-infra",
    name: "Worker Infrastructure",
    docType: "spec",
    filePath: null,
    linkedNodeCount: 0,
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: "2024-12-11T12:00:00Z",
    createdAt: "2024-12-11T12:00:00Z",
    updatedAt: "2024-12-11T12:00:00Z",
    title: "Worker Infrastructure",
    category: "spec",
    content:
      "# Worker Infrastructure\n\nWorkers are ephemeral compute instances that execute tasks assigned by the task dispatcher. Each worker reports CPU/memory usage and current task assignment via WebSocket heartbeats.",
    preview: "Ephemeral compute instances with WebSocket heartbeats",
    folder: "Specifications",
  },
];

// ---------------------------------------------------------------------------
// 21. File Trees
// ---------------------------------------------------------------------------

export const MOCK_FILE_TREES: Record<string, FileTreeEntry[]> = {
  "branch-tgr-common": [
    {
      name: "src",
      path: "src",
      type: "directory",
      size: null,
      children: [
        {
          name: "App.tsx",
          path: "src/App.tsx",
          type: "file",
          size: 3200,
        },
        {
          name: "main.tsx",
          path: "src/main.tsx",
          type: "file",
          size: 2400,
        },
        {
          name: "index.css",
          path: "src/index.css",
          type: "file",
          size: 18200,
        },
        {
          name: "pages",
          path: "src/pages",
          type: "directory",
          size: null,
          children: [
            {
              name: "WorkspacePage.tsx",
              path: "src/pages/WorkspacePage.tsx",
              type: "file",
              size: 42000,
            },
            {
              name: "LandingPage.tsx",
              path: "src/pages/LandingPage.tsx",
              type: "file",
              size: 8600,
            },
            {
              name: "SettingsPage.tsx",
              path: "src/pages/SettingsPage.tsx",
              type: "file",
              size: 5400,
            },
          ],
        },
        {
          name: "components",
          path: "src/components",
          type: "directory",
          size: null,
          children: [
            {
              name: "tree",
              path: "src/components/tree",
              type: "directory",
              size: null,
              children: [
                {
                  name: "TreeNode.tsx",
                  path: "src/components/tree/TreeNode.tsx",
                  type: "file",
                  size: 28800,
                },
                {
                  name: "OntologyTree.tsx",
                  path: "src/components/tree/OntologyTree.tsx",
                  type: "file",
                  size: 12400,
                },
                {
                  name: "FileTree.tsx",
                  path: "src/components/tree/FileTree.tsx",
                  type: "file",
                  size: 5600,
                },
              ],
            },
            {
              name: "workspace",
              path: "src/components/workspace",
              type: "directory",
              size: null,
              children: [
                {
                  name: "TopBar.tsx",
                  path: "src/components/workspace/TopBar.tsx",
                  type: "file",
                  size: 15200,
                },
                {
                  name: "CenterPanel.tsx",
                  path: "src/components/workspace/CenterPanel.tsx",
                  type: "file",
                  size: 18600,
                },
                {
                  name: "RightPanel.tsx",
                  path: "src/components/workspace/RightPanel.tsx",
                  type: "file",
                  size: 12400,
                },
                {
                  name: "BottomDrawer.tsx",
                  path: "src/components/workspace/BottomDrawer.tsx",
                  type: "file",
                  size: 8200,
                },
                {
                  name: "ReposBar.tsx",
                  path: "src/components/workspace/ReposBar.tsx",
                  type: "file",
                  size: 3800,
                },
              ],
            },
            {
              name: "settings",
              path: "src/components/settings",
              type: "directory",
              size: null,
              children: [
                {
                  name: "UserSettingsPanel.tsx",
                  path: "src/components/settings/UserSettingsPanel.tsx",
                  type: "file",
                  size: 28400,
                },
                {
                  name: "SettingsForm.tsx",
                  path: "src/components/settings/SettingsForm.tsx",
                  type: "file",
                  size: 8800,
                },
                {
                  name: "SpendBreakdown.tsx",
                  path: "src/components/settings/SpendBreakdown.tsx",
                  type: "file",
                  size: 6200,
                },
              ],
            },
            {
              name: "ui",
              path: "src/components/ui",
              type: "directory",
              size: null,
              children: [
                {
                  name: "UI.tsx",
                  path: "src/components/ui/UI.tsx",
                  type: "file",
                  size: 9600,
                },
              ],
            },
          ],
        },
        {
          name: "lib",
          path: "src/lib",
          type: "directory",
          size: null,
          children: [
            {
              name: "types.ts",
              path: "src/lib/types.ts",
              type: "file",
              size: 32000,
            },
            {
              name: "api.ts",
              path: "src/lib/api.ts",
              type: "file",
              size: 8400,
            },
            {
              name: "utils.ts",
              path: "src/lib/utils.ts",
              type: "file",
              size: 4200,
            },
            {
              name: "auth.ts",
              path: "src/lib/auth.ts",
              type: "file",
              size: 3600,
            },
          ],
        },
        {
          name: "data",
          path: "src/data",
          type: "directory",
          size: null,
          children: [
            {
              name: "mock.ts",
              path: "src/data/mock.ts",
              type: "file",
              size: 86000,
            },
            {
              name: "handlers.ts",
              path: "src/data/handlers.ts",
              type: "file",
              size: 12400,
            },
          ],
        },
        {
          name: "hooks",
          path: "src/hooks",
          type: "directory",
          size: null,
          children: [
            {
              name: "useOntology.ts",
              path: "src/hooks/useOntology.ts",
              type: "file",
              size: 4800,
            },
            {
              name: "useSettings.ts",
              path: "src/hooks/useSettings.ts",
              type: "file",
              size: 3200,
            },
            {
              name: "useChat.ts",
              path: "src/hooks/useChat.ts",
              type: "file",
              size: 5600,
            },
          ],
        },
      ],
    },
    {
      name: "package.json",
      path: "package.json",
      type: "file",
      size: 1200,
    },
    {
      name: "vite.config.ts",
      path: "vite.config.ts",
      type: "file",
      size: 1800,
    },
    {
      name: "tailwind.config.ts",
      path: "tailwind.config.ts",
      type: "file",
      size: 600,
    },
    {
      name: "tsconfig.json",
      path: "tsconfig.json",
      type: "file",
      size: 400,
    },
  ],
  "branch-tgr-api": [
    {
      name: "src",
      path: "src",
      type: "directory",
      size: null,
      children: [
        {
          name: "main.rs",
          path: "src/main.rs",
          type: "file",
          size: 3200,
        },
        {
          name: "config.rs",
          path: "src/config.rs",
          type: "file",
          size: 2400,
        },
        {
          name: "db.rs",
          path: "src/db.rs",
          type: "file",
          size: 4800,
        },
        {
          name: "errors.rs",
          path: "src/errors.rs",
          type: "file",
          size: 3600,
        },
        {
          name: "routes",
          path: "src/routes",
          type: "directory",
          size: null,
          children: [
            {
              name: "mod.rs",
              path: "src/routes/mod.rs",
              type: "file",
              size: 1200,
            },
            {
              name: "auth.rs",
              path: "src/routes/auth.rs",
              type: "file",
              size: 5200,
            },
            {
              name: "workspaces.rs",
              path: "src/routes/workspaces.rs",
              type: "file",
              size: 8400,
            },
            {
              name: "tasks.rs",
              path: "src/routes/tasks.rs",
              type: "file",
              size: 5800,
            },
            {
              name: "ontology.rs",
              path: "src/routes/ontology.rs",
              type: "file",
              size: 7200,
            },
            {
              name: "search.rs",
              path: "src/routes/search.rs",
              type: "file",
              size: 4200,
            },
            {
              name: "settings.rs",
              path: "src/routes/settings.rs",
              type: "file",
              size: 3800,
            },
            {
              name: "chat.rs",
              path: "src/routes/chat.rs",
              type: "file",
              size: 5600,
            },
          ],
        },
        {
          name: "services",
          path: "src/services",
          type: "directory",
          size: null,
          children: [
            {
              name: "mod.rs",
              path: "src/services/mod.rs",
              type: "file",
              size: 800,
            },
            {
              name: "workspace_service.rs",
              path: "src/services/workspace_service.rs",
              type: "file",
              size: 9200,
            },
            {
              name: "task_service.rs",
              path: "src/services/task_service.rs",
              type: "file",
              size: 6400,
            },
            {
              name: "ontology_service.rs",
              path: "src/services/ontology_service.rs",
              type: "file",
              size: 8600,
            },
            {
              name: "search_service.rs",
              path: "src/services/search_service.rs",
              type: "file",
              size: 5200,
            },
            {
              name: "user_service.rs",
              path: "src/services/user_service.rs",
              type: "file",
              size: 4800,
            },
          ],
        },
        {
          name: "auth",
          path: "src/auth",
          type: "directory",
          size: null,
          children: [
            {
              name: "mod.rs",
              path: "src/auth/mod.rs",
              type: "file",
              size: 600,
            },
            {
              name: "jwt.rs",
              path: "src/auth/jwt.rs",
              type: "file",
              size: 5800,
            },
            {
              name: "oauth.rs",
              path: "src/auth/oauth.rs",
              type: "file",
              size: 7200,
            },
            {
              name: "scm.rs",
              path: "src/auth/scm.rs",
              type: "file",
              size: 4200,
            },
          ],
        },
      ],
    },
    {
      name: "Cargo.toml",
      path: "Cargo.toml",
      type: "file",
      size: 1400,
    },
  ],
  "branch-auth-api": [
    {
      name: "src",
      path: "src",
      type: "directory",
      size: null,
      children: [
        {
          name: "main.rs",
          path: "src/main.rs",
          type: "file",
          size: 2800,
        },
        {
          name: "agent.rs",
          path: "src/agent.rs",
          type: "file",
          size: 12400,
        },
        {
          name: "worker.rs",
          path: "src/worker.rs",
          type: "file",
          size: 9600,
        },
        {
          name: "config.rs",
          path: "src/config.rs",
          type: "file",
          size: 3200,
        },
        {
          name: "tools",
          path: "src/tools",
          type: "directory",
          size: null,
          children: [
            {
              name: "mod.rs",
              path: "src/tools/mod.rs",
              type: "file",
              size: 2400,
            },
            {
              name: "file_ops.rs",
              path: "src/tools/file_ops.rs",
              type: "file",
              size: 6800,
            },
            {
              name: "git_ops.rs",
              path: "src/tools/git_ops.rs",
              type: "file",
              size: 5200,
            },
            {
              name: "search.rs",
              path: "src/tools/search.rs",
              type: "file",
              size: 4600,
            },
            {
              name: "terminal.rs",
              path: "src/tools/terminal.rs",
              type: "file",
              size: 7200,
            },
            {
              name: "spec_ops.rs",
              path: "src/tools/spec_ops.rs",
              type: "file",
              size: 5800,
            },
            {
              name: "send_message.rs",
              path: "src/tools/send_message.rs",
              type: "file",
              size: 3400,
            },
            {
              name: "task_lifecycle.rs",
              path: "src/tools/task_lifecycle.rs",
              type: "file",
              size: 4200,
            },
            {
              name: "interface.rs",
              path: "src/tools/interface.rs",
              type: "file",
              size: 5600,
            },
          ],
        },
        {
          name: "llm",
          path: "src/llm",
          type: "directory",
          size: null,
          children: [
            {
              name: "mod.rs",
              path: "src/llm/mod.rs",
              type: "file",
              size: 1800,
            },
            {
              name: "anthropic.rs",
              path: "src/llm/anthropic.rs",
              type: "file",
              size: 6400,
            },
            {
              name: "openai.rs",
              path: "src/llm/openai.rs",
              type: "file",
              size: 5800,
            },
            {
              name: "openrouter.rs",
              path: "src/llm/openrouter.rs",
              type: "file",
              size: 4200,
            },
            {
              name: "http.rs",
              path: "src/llm/http.rs",
              type: "file",
              size: 3600,
            },
          ],
        },
      ],
    },
    {
      name: "Cargo.toml",
      path: "Cargo.toml",
      type: "file",
      size: 1200,
    },
  ],
};

// ---------------------------------------------------------------------------
// 22. Rust Source File Contents (13 files)
// ---------------------------------------------------------------------------

export const MOCK_FILE_CONTENT: Record<string, string> = {
  "branch-tgr-common:src/pages/WorkspacePage.tsx": `import React, { useState, useCallback, useMemo } from 'react';
import { useParams } from 'react-router-dom';
import { useWorkspace, useOntology, useTasks } from '../hooks';
import { LeftSidebar } from '../components/tree/TreeNode';
import { TopBar } from '../components/workspace/TopBar';
import { CenterPanel } from '../components/workspace/CenterPanel';
import { RightPanel } from '../components/workspace/RightPanel';
import { BottomDrawer } from '../components/workspace/BottomDrawer';
import { UserSettingsPanel } from '../components/settings/UserSettingsPanel';
import type { OntologyNodeDTO } from '../lib/types';

/**
 * WorkspacePage - Shell layout with 5 resizable panels.
 *
 * Layout: LeftSidebar | CenterPanel | RightPanel
 *                    BottomDrawer
 */
export function WorkspacePage() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const { workspace, branches, activeBranch } = useWorkspace(workspaceId!);

  // Stacked sidebar layout - no tabs
  const [sidebarSections, setSidebarSections] = useState({
    docs: true, ontology: true, tasks: true, files: false,
  });
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [selectedNode, setSelectedNode] = useState<OntologyNodeDTO | null>(null);
  const [bottomDrawerOpen, setBottomDrawerOpen] = useState(false);

  const handleNodeSelect = useCallback((node: OntologyNodeDTO) => {
    setSelectedNode(node);
  }, []);

  if (!workspace) return <div>Loading workspace...</div>;

  return (
    <div className="flex flex-col h-screen bg-[var(--g5-bg0)] text-[var(--g5-text0)]">
      <TopBar
        workspace={workspace}
        branches={branches}
        activeBranch={activeBranch}
        onSettingsOpen={() => setSettingsOpen(true)}
      />
      <div className="flex flex-1 overflow-hidden">
        <LeftSidebar
          branchId={activeBranch?.repoId}
          sections={sidebarSections}
          onToggleSection={(key) =>
            setSidebarSections((s) => ({ ...s, [key]: !s[key] }))
          }
          onNodeSelect={handleNodeSelect}
          selectedNodeId={selectedNode?.id ?? null}
        />
        <CenterPanel
          node={selectedNode}
          branchId={activeBranch?.repoId}
        />
        <RightPanel node={selectedNode} />
      </div>
      {bottomDrawerOpen && <BottomDrawer />}
      {settingsOpen && (
        <UserSettingsPanel onClose={() => setSettingsOpen(false)} />
      )}
    </div>
  );
}`,

  "branch-tgr-common:src/components/tree/TreeNode.tsx": `import React, { useState, useCallback } from 'react';
import {
  ChevronRight, ChevronDown, FileCode, FolderTree,
  BookOpen, Package, AlertCircle, GitCommit,
  CheckCircle2, Clock, Plus, Trash2, Layers,
} from 'lucide-react';
import type { OntologyNodeDTO } from '../../lib/types';

const SYNC_STATUS_COLORS = {
  synced: 'bg-emerald-400',
  modified: 'bg-amber-400',
  added: 'bg-blue-400',
  deleted: 'bg-red-400',
  conflict: 'bg-purple-400',
};

function nodeIcon(nodeType: string) {
  switch (nodeType) {
    case 'code': return <FileCode className="w-4 h-4" />;
    case 'document': return <BookOpen className="w-4 h-4" />;
    default: return <FolderTree className="w-4 h-4" />;
  }
}

interface TreeNodeProps {
  node: OntologyNodeDTO;
  depth: number;
  isSelected: boolean;
  onSelect: (node: OntologyNodeDTO) => void;
  onExpand: (nodeId: string) => void;
  children?: OntologyNodeDTO[];
}

export function TreeNodeItem({ node, depth, isSelected, onSelect, onExpand, children }: TreeNodeProps) {
  const [expanded, setExpanded] = useState(false);
  const hasChildren = (node.childCount ?? 0) > 0;

  const toggle = useCallback(() => {
    if (hasChildren) {
      setExpanded((e) => !e);
      onExpand(node.id);
    }
  }, [hasChildren, node.id, onExpand]);

  return (
    <div>
      <div
        className={\`flex items-center gap-1.5 px-2 py-1 text-sm cursor-pointer
          hover:bg-[var(--g5-bg1)] rounded transition-colors
          \${isSelected ? 'bg-[var(--g5-active)]/10 text-[var(--g5-active)]' : 'text-[var(--g5-text0)]'}
        \`}
        style={{ paddingLeft: \`\${depth * 16 + 8}px\` }}
        onClick={() => { onSelect(node); toggle(); }}
      >
        {hasChildren ? (
          expanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />
        ) : <span className="w-3.5" />}
        {nodeIcon(node.nodeType)}
        <span className="truncate flex-1">{node.name}</span>
        <span className={\`w-2 h-2 rounded-full \${SYNC_STATUS_COLORS[node.syncStatus]}\`} />
      </div>
      {expanded && children?.map((child) => (
        <TreeNodeItem
          key={child.id} node={child} depth={depth + 1}
          isSelected={false} onSelect={onSelect} onExpand={onExpand}
        />
      ))}
    </div>
  );
}`,

  "branch-tgr-common:src/lib/types.ts": `// G5 Client TypeScript type definitions

export interface OntologyNodeDTO {
  id: string;
  workspaceId: string;
  parentId: string | null;
  name: string;
  nodeType: 'node' | 'document' | 'code';
  filePath?: string;
  sortOrder: number;
  syncStatus: 'synced' | 'modified' | 'added' | 'deleted' | 'conflict';
  aheadOfBase: number;
  behindBase: number;
  childCount: number;
}

export interface WorkspaceDTO {
  id: string;
  name: string;
  repoFullName: string;
  status: 'active' | 'paused' | 'archived';
  currentTaskId: string | null;
}

export interface EditorSettings {
  editorFontSize: number;
  editorTabSize: number;
  rainbowBrackets: boolean;
  codeFolding: boolean;
  showMinimap: boolean;
  wordWrap: 'off' | 'on' | 'wordWrapColumn';
  indentGuides: boolean;
  bracketPairGuides: boolean;
  ligatures: boolean;
  stickyScroll: boolean;
  renderWhitespace: 'none' | 'boundary' | 'all';
}

export interface UserSettings {
  theme: string;
  mode: 'light' | 'dark';
  preferredModel: string;
  monthlyBudgetUsd: number;
  notifications: boolean;
  editor: EditorSettings;
}`,

  "branch-tgr-common:src/index.css": `/* G5 Theme System - Tailwind v4 */
@import "tailwindcss";

/* 10 themes × 2 modes = 20 palettes */
/* Each palette defines:
 * --g5-bg, --g5-fg, --g5-surface-0..2, --g5-active, --g5-border
 * --g5-editor-bg, --g5-editor-gutter, --g5-editor-line-active
 * --g5-syntax-keyword..string..comment..function..type..number..operator
 * --g5-bracket-1..2..3, --g5-shadow
 */

:root[data-theme="editor"][data-mode="dark"] {
  --g5-bg: #1e1e2e;
  --g5-fg: #cdd6f4;
  --g5-surface-0: #181825;
  --g5-surface-1: #313244;
  --g5-surface-2: #45475a;
  --g5-active: #89b4fa;
  --g5-border: #585b70;
  /* ... 25 tokens per palette */
}`,

  "branch-auth-api:src/auth/jwt.rs": `use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JWT claims structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,        // User ID
    pub org_id: String,     // Organization ID
    pub role: String,       // User role
    pub exp: u64,           // Expiration (15 min for access)
    pub iat: u64,           // Issued at
    pub nbf: u64,           // Not before
    pub jti: String,        // JWT ID (for revocation)
}

/// JWT service for token operations using RS256.
pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_ttl_secs: u64,
    refresh_ttl_secs: u64,
}

impl JwtService {
    pub fn new(private_key: &[u8], public_key: &[u8]) -> Result<Self, JwtError> {
        Ok(Self {
            encoding_key: EncodingKey::from_rsa_pem(private_key)
                .map_err(|e| JwtError::KeyError(e.to_string()))?,
            decoding_key: DecodingKey::from_rsa_pem(public_key)
                .map_err(|e| JwtError::KeyError(e.to_string()))?,
            access_ttl_secs: 900,
            refresh_ttl_secs: 604800,
        })
    }

    pub fn sign_access(&self, user_id: &str, org_id: &str, role: &str) -> Result<String, JwtError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let claims = Claims {
            sub: user_id.to_string(), org_id: org_id.to_string(),
            role: role.to_string(), exp: now + self.access_ttl_secs,
            iat: now, nbf: now, jti: uuid::Uuid::new_v4().to_string(),
        };
        let header = Header::new(Algorithm::RS256);
        encode(&header, &claims, &self.encoding_key)
            .map_err(|e| JwtError::SigningError(e.to_string()))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, JwtError> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 30;
        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|e| JwtError::ValidationError(e.to_string()))
    }
}

#[derive(Debug)]
pub enum JwtError { KeyError(String), SigningError(String), ValidationError(String) }`,

  "branch-auth-api:src/agent.rs": `use crate::tools::ToolRegistry;
use crate::llm::{LlmClient, LlmProvider, ModelConfig};
use crate::worker::WorkerHandle;

/// Main agent orchestrator that manages the agentic loop.
///
/// The agent follows a ReAct pattern:
/// 1. Receive task assignment from API
/// 2. Load context (spec, source files, tests)
/// 3. Enter agentic loop: Think → Act → Observe
/// 4. Commit changes and report completion
pub struct Agent {
    tools: ToolRegistry,
    llm: Box<dyn LlmProvider>,
    worker: WorkerHandle,
    max_iterations: usize,
    iteration: usize,
}

impl Agent {
    pub fn new(
        tools: ToolRegistry,
        llm: Box<dyn LlmProvider>,
        worker: WorkerHandle,
        max_iterations: usize,
    ) -> Self {
        Self { tools, llm, worker, max_iterations, iteration: 0 }
    }

    /// Run the agentic loop for a task.
    pub async fn execute_task(&mut self, task_id: &str) -> Result<TaskResult, AgentError> {
        let task = self.worker.fetch_task(task_id).await?;
        let spec = self.tools.read_spec(&task.node_id).await?;

        while self.iteration < self.max_iterations {
            self.iteration += 1;

            // Think: ask LLM for next action
            let response = self.llm.complete(&self.build_prompt(&spec)).await?;

            // Act: execute tool calls
            for tool_call in response.tool_calls {
                let result = self.tools.execute(&tool_call).await?;
                // Observe: feed result back
                self.worker.log_timeline(task_id, &result).await?;
            }

            if response.is_complete {
                return Ok(TaskResult::Completed);
            }
        }
        Err(AgentError::MaxIterationsExceeded)
    }
}

pub enum TaskResult { Completed, NeedsReview }
pub enum AgentError { MaxIterationsExceeded, LlmError(String), ToolError(String) }`,

  "branch-auth-api:src/tools/file_ops.rs": `use std::path::PathBuf;
use tokio::fs;

/// File operations tool for the agent.
pub struct FileOpsTool {
    workspace_root: PathBuf,
}

impl FileOpsTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Read a file relative to the workspace root.
    pub async fn read_file(&self, path: &str) -> Result<String, ToolError> {
        let full_path = self.workspace_root.join(path);
        fs::read_to_string(&full_path)
            .await
            .map_err(|e| ToolError::IoError(e.to_string()))
    }

    /// Write content to a file.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), ToolError> {
        let full_path = self.workspace_root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| ToolError::IoError(e.to_string()))?;
        }
        fs::write(&full_path, content)
            .await
            .map_err(|e| ToolError::IoError(e.to_string()))
    }
}

pub enum ToolError { IoError(String), NotFound(String) }`,
};

// ---------------------------------------------------------------------------
// 24. Workspace Settings
// ---------------------------------------------------------------------------

export const MOCK_WORKSPACE_SETTINGS: Record<string, WorkspaceSettings> = {
  "task-graph-rework": {
    workspaceId: "task-graph-rework",
    maxSpendUsd: 100,
    spendPeriod: "daily",
    spendPeriodStart: "2024-12-15T00:00:00Z",
    maxInputTokens: null,
    maxOutputTokens: null,
    idleTimeoutSecs: 1800,
    responseDeadlineSecs: 900,
    boundedExecutionSlots: 2,
    permissionPolicy: null,
    agentMayModifyOtherWorkspaces: true,
    updatedBy: "user-alex",
    updatedAt: "2024-12-15T12:00:00Z",
  },
  "auth-refactor": {
    workspaceId: "auth-refactor",
    maxSpendUsd: 50,
    spendPeriod: "weekly",
    spendPeriodStart: "2024-12-09T00:00:00Z",
    maxInputTokens: null,
    maxOutputTokens: null,
    idleTimeoutSecs: 900,
    responseDeadlineSecs: 900,
    boundedExecutionSlots: 2,
    permissionPolicy: null,
    agentMayModifyOtherWorkspaces: true,
    updatedBy: "user-jordan",
    updatedAt: "2024-12-14T10:00:00Z",
  },
  "new-feature-idea": {
    workspaceId: "new-feature-idea",
    maxSpendUsd: null,
    spendPeriod: "monthly",
    spendPeriodStart: "2024-12-01T00:00:00Z",
    maxInputTokens: null,
    maxOutputTokens: null,
    idleTimeoutSecs: 600,
    responseDeadlineSecs: 900,
    boundedExecutionSlots: 2,
    permissionPolicy: null,
    agentMayModifyOtherWorkspaces: true,
    updatedBy: "user-taylor",
    updatedAt: "2024-12-10T16:00:00Z",
  },
};

// ---------------------------------------------------------------------------
// 25. Workspace Spend
// ---------------------------------------------------------------------------

export const MOCK_WORKSPACE_SPEND: Record<string, SpendBreakdownData> = {
  "task-graph-rework": {
    periodStart: "2024-12-15T00:00:00Z",
    periodEnd: "2024-12-16T00:00:00Z",
    totalSpendUsd: 45.2,
    limitSpendUsd: 100,
    totalInputTokens: 7_700_000,
    totalOutputTokens: 2_400_000,
    limitInputTokens: null,
    limitOutputTokens: null,
    isSpendLimitReached: false,
    unattributedSpendUsd: 0,
    otherTaskSpendUsd: 0,
    byTask: [
      {
        taskId: "task-spend-1",

        taskType: "agent",

        taskName: "Compile workspace specs",

        spendUsd: 8.4,
        inputTokens: 0,
        outputTokens: 0,
      },
      {
        taskId: "task-spend-2",

        taskType: "agent",

        taskName: "Uplift core modules",

        spendUsd: 22.3,
        inputTokens: 0,
        outputTokens: 0,
      },
      {
        taskId: "task-spend-3",

        taskType: "agent",

        taskName: "Downlift API layer",

        spendUsd: 0,
        inputTokens: 0,
        outputTokens: 0,
      },
    ],
  },
  "auth-refactor": {
    periodStart: "2024-12-09T00:00:00Z",
    periodEnd: "2024-12-16T00:00:00Z",
    totalSpendUsd: 12.8,
    limitSpendUsd: 50,
    totalInputTokens: 2_300_000,
    totalOutputTokens: 780_000,
    limitInputTokens: null,
    limitOutputTokens: null,
    isSpendLimitReached: false,
    unattributedSpendUsd: 0,
    otherTaskSpendUsd: 0,
    byTask: [
      {
        taskId: "task-spend-4",

        taskType: "agent",

        taskName: "Uplift auth modules",

        spendUsd: 6.5,
        inputTokens: 0,
        outputTokens: 0,
      },
      {
        taskId: "task-spend-5",

        taskType: "agent",

        taskName: "Downlift JWT implementation",

        spendUsd: 6.3,
        inputTokens: 0,
        outputTokens: 0,
      },
    ],
  },
  "new-feature-idea": {
    periodStart: "2024-12-01T00:00:00Z",
    periodEnd: "2025-01-01T00:00:00Z",
    totalSpendUsd: 0,
    limitSpendUsd: null,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    limitInputTokens: null,
    limitOutputTokens: null,
    isSpendLimitReached: false,
    unattributedSpendUsd: 0,
    otherTaskSpendUsd: 0,
    byTask: [],
  },
};

// ---------------------------------------------------------------------------
// 26. Spend History
// ---------------------------------------------------------------------------

export const MOCK_SPEND_HISTORY: Record<string, SpendHistoryDTO> = {
  "task-graph-rework": {
    granularity: "daily",
    periods: [
      {
        periodStart: "2024-12-14T00:00:00Z",
        periodEnd: "2024-12-15T00:00:00Z",
        totalSpendUsd: 14.5,
        totalInputTokens: 2_500_000,
        totalOutputTokens: 800_000,

        taskCount: 4,
      },
      {
        periodStart: "2024-12-15T00:00:00Z",
        periodEnd: "2024-12-16T00:00:00Z",
        totalSpendUsd: 30.7,
        totalInputTokens: 5_200_000,
        totalOutputTokens: 1_600_000,

        taskCount: 8,
      },
    ],
    daily: [
      {
        date: "2024-12-14",
        spendUsd: 14.5,
        inputTokens: 2_500_000,
        outputTokens: 800_000,
      },
      {
        date: "2024-12-15",
        spendUsd: 30.7,
        inputTokens: 5_200_000,
        outputTokens: 1_600_000,
      },
    ],
  },
  "auth-refactor": {
    granularity: "daily",
    periods: [
      {
        periodStart: "2024-12-12T00:00:00Z",
        periodEnd: "2024-12-13T00:00:00Z",
        totalSpendUsd: 6.5,
        totalInputTokens: 1_200_000,
        totalOutputTokens: 400_000,

        taskCount: 4,
      },
      {
        periodStart: "2024-12-13T00:00:00Z",
        periodEnd: "2024-12-14T00:00:00Z",
        totalSpendUsd: 6.3,
        totalInputTokens: 1_100_000,
        totalOutputTokens: 380_000,

        taskCount: 4,
      },
    ],
    daily: [
      {
        date: "2024-12-12",
        spendUsd: 6.5,
        inputTokens: 1_200_000,
        outputTokens: 400_000,
      },
      {
        date: "2024-12-13",
        spendUsd: 6.3,
        inputTokens: 1_100_000,
        outputTokens: 380_000,
      },
    ],
  },
  "new-feature-idea": {
    granularity: "daily",
    periods: [],
    daily: [],
  },
};

// ---------------------------------------------------------------------------
// 29. Diff Lines (~28 lines — HashMap→BTreeMap refactor)
// ---------------------------------------------------------------------------

export const DIFF_LINES: DiffLine[] = [
  {
    type: "context",
    content: "use std::collections::HashMap;",
    oldLine: 1,
    newLine: 1,
  },
  { type: "deletion", content: "use std::collections::HashMap;", oldLine: 1 },
  { type: "addition", content: "use std::collections::BTreeMap;", newLine: 1 },
  { type: "context", content: "", oldLine: 2, newLine: 2 },
  {
    type: "context",
    content: "/// Task registry with ordered iteration.",
    oldLine: 3,
    newLine: 3,
  },
  {
    type: "context",
    content: "pub struct TaskRegistry {",
    oldLine: 4,
    newLine: 4,
  },
  {
    type: "deletion",
    content: "    tasks: HashMap<String, Task>,",
    oldLine: 5,
  },
  {
    type: "addition",
    content: "    tasks: BTreeMap<String, Task>,",
    newLine: 5,
  },
  {
    type: "deletion",
    content: "    metadata: HashMap<String, Metadata>,",
    oldLine: 6,
  },
  {
    type: "addition",
    content: "    metadata: BTreeMap<String, Metadata>,",
    newLine: 6,
  },
  { type: "context", content: "}", oldLine: 7, newLine: 7 },
  { type: "context", content: "", oldLine: 8, newLine: 8 },
  { type: "context", content: "impl TaskRegistry {", oldLine: 9, newLine: 9 },
  {
    type: "context",
    content: "    pub fn new() -> Self {",
    oldLine: 10,
    newLine: 10,
  },
  { type: "context", content: "        Self {", oldLine: 11, newLine: 11 },
  {
    type: "deletion",
    content: "            tasks: HashMap::new(),",
    oldLine: 12,
  },
  {
    type: "addition",
    content: "            tasks: BTreeMap::new(),",
    newLine: 12,
  },
  {
    type: "deletion",
    content: "            metadata: HashMap::new(),",
    oldLine: 13,
  },
  {
    type: "addition",
    content: "            metadata: BTreeMap::new(),",
    newLine: 13,
  },
  { type: "context", content: "        }", oldLine: 14, newLine: 14 },
  { type: "context", content: "    }", oldLine: 15, newLine: 15 },
  { type: "context", content: "", oldLine: 16, newLine: 16 },
  {
    type: "addition",
    content: "    /// Iterate tasks in sorted key order.",
    newLine: 17,
  },
  {
    type: "addition",
    content:
      "    pub fn iter_sorted(&self) -> impl Iterator<Item = (&String, &Task)> {",
    newLine: 18,
  },
  { type: "addition", content: "        self.tasks.iter()", newLine: 19 },
  { type: "addition", content: "    }", newLine: 20 },
  { type: "addition", content: "", newLine: 21 },
  {
    type: "context",
    content: "    pub fn insert(&mut self, id: String, task: Task) {",
    oldLine: 17,
    newLine: 22,
  },
];

// ---------------------------------------------------------------------------
// 30. Terminal Output (~18 lines)
// ---------------------------------------------------------------------------

export const TERMINAL_OUTPUT: string[] = [
  "$ cargo build --release",
  "   Compiling g5-common v0.1.0 (/workspace/g5-common)",
  "   Compiling g5-api v0.1.0 (/workspace/g5-api)",
  "    Finished release [optimized] target(s) in 45.2s",
  "",
  "$ cargo test",
  "   Compiling g5-common v0.1.0 (/workspace/g5-common)",
  "   Compiling g5-api v0.1.0 (/workspace/g5-api)",
  "    Finished test [unoptimized + debuginfo] target(s) in 32.1s",
  "     Running unittests src/lib.rs (target/debug/deps/g5_common-abc123)",
  "",
  "running 24 tests",
  "test scheduler::test_next_task ... ok",
  "test scheduler::test_complete_task ... ok",
  "test dag::test_topological_sort ... ok",
  "test dag::test_cycle_detection ... ok",
  "test allocator::test_resource_allocation ... ok",
  "test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.85s",
];

// ---------------------------------------------------------------------------
// 31. Theme Options (10 themes)
// ---------------------------------------------------------------------------

export const THEME_OPTIONS: {
  name: ThemeName;
  label: string;
  accent: string;
}[] = [
  { name: "editor", label: "Editor", accent: "#3b82f6" },
  { name: "signal-studio", label: "Signal Studio", accent: "#8b5cf6" },
  { name: "obsidian-gold", label: "Obsidian & Gold", accent: "#d97706" },
  { name: "carbon-clay", label: "Carbon Clay", accent: "#ef4444" },
  { name: "alpine-graphite", label: "Alpine Graphite", accent: "#059669" },
  { name: "darkroom", label: "Darkroom", accent: "#f59e0b" },
  { name: "ink-parchment", label: "Ink & Parchment", accent: "#78716c" },
  { name: "studio-night", label: "Studio Night", accent: "#ec4899" },
  { name: "pm-paper", label: "PM Paper", accent: "#6366f1" },
  { name: "green-room", label: "Green room", accent: "#1ed760" },
  { name: "classic-98", label: "Old school", accent: "#000080" },
  { name: "aero-glass", label: "Aero Glass", accent: "#4da3ff" },
];

// ---------------------------------------------------------------------------
// 32. Default Settings
// ---------------------------------------------------------------------------

export const DEFAULT_SETTINGS: UserSettings = {
  themeName: "editor",
  themeMode: "dark",
  defaultAgentProviderChoices: {
    claude_code: {
      model: "anthropic/claude-opus-4.8",
      mode: "bypassPermissions",
      effort: "high",
    },
    codex: {
      model: "gpt-5.5",
      mode: "never/danger-full-access",
      effort: "xhigh",
    },
  },
  sidebarDisplayMode: "collapse-to-rail",
  subscriptionUsageWindow: "7d",
  phaseBarStyle: "minimal",
  actionBarStyle: "dynamic",
  showTabBar: false,
  showGuidedJourney: false,
  homeRowsPerTile: 5,
  chatBubbleStyle: "3d",
  chatBubbleLimit: 5,
  budgetWarnAt: 80,
  editor: {
    editorFontSize: 14,
    editorTabSize: 4,
    rainbowBrackets: true,
    codeFolding: true,
    showMinimap: false,
    wordWrap: "off",
    indentGuides: true,
    bracketPairGuides: true,
    ligatures: false,
    stickyScroll: true,
    renderWhitespace: "selection",
  },
};

// ---------------------------------------------------------------------------
// 34. Instance Info & Health
// ---------------------------------------------------------------------------

export const MOCK_INSTANCE_INFO: InstanceInfoDTO = {
  version: "0.1.0",
  // Mirror the standard Codex capability advertised to API clients.
  features: ["chat", "codex_ui"],
  // The classic combined header, as CI pins it; the client fallback for an
  // absent value is `alignment` (workbench g5.workbench.gate), so suites that
  // rely on the status band keep an explicit default here.
  authProviders: ["github"],
  maxUploadSizeMb: 100,
  selfHosted: false,
  devLoginEnabled: true,
  motd: "Welcome to G5! 🚀 Version 0.1.0 is now available.",
};

export const MOCK_INSTANCE_HEALTH: HealthStatus = {
  status: "healthy",
  checks: {
    database: "healthy",
  },
  uptime: 864000,
};

// ---------------------------------------------------------------------------
// 35. Chat Models
// ---------------------------------------------------------------------------

export const MOCK_CHAT_MODELS: string[] = [
  // Keep the refreshed default selectable in dev/test mock mode so it matches
  // the bundled chat default ("anthropic/claude-opus-5.5"); the legacy ids
  // below remain for existing message fixtures.
  "anthropic/claude-opus-5.5",
  "anthropic/claude-opus-4.8",
  "claude-opus-4-8",
  "claude-sonnet-4-20250514",
  "claude-3-5-haiku-20241022",
  "claude-3-opus-20240229",
];

// ---------------------------------------------------------------------------
// Lookup Helper Functions
// ---------------------------------------------------------------------------

export function getMockWorkspace(
  workspaceId: string,
): WorkspaceSummary | undefined {
  return MOCK_WORKSPACES.find((w) => w.id === workspaceId);
}

export function getMockTask(taskId: string): TaskDetail | undefined {
  return MOCK_TASKS[taskId];
}

export function getMockChatSessions(workspaceId: string): ChatSessionDTO[] {
  return MOCK_CHAT_SESSIONS[workspaceId] ?? [];
}

export function getMockChatMessages(sessionId: string): ChatMessageDTO[] {
  return MOCK_CHAT_MESSAGES[sessionId] ?? [];
}

export function getMockComments(nodeId: string): CommentDTO[] {
  return MOCK_COMMENTS[nodeId] ?? [];
}

export function getMockNotifications(workspaceId: string): G5Notification[] {
  return MOCK_NOTIFICATIONS[workspaceId] ?? [];
}

export function getMockFileTree(branchId: string): FileTreeEntry[] {
  return MOCK_FILE_TREES[branchId] ?? [];
}

export function getMockFileContent(
  branchId: string,
  path: string,
): string | undefined {
  return MOCK_FILE_CONTENT[`${branchId}:${path}`];
}

export function getMockWorkspaceSettings(
  workspaceId: string,
): WorkspaceSettings | undefined {
  return MOCK_WORKSPACE_SETTINGS[workspaceId];
}
