// =============================================================================
// G5 API Contract Schemas — Zod schemas that describe the EXACT JSON shapes
// returned by the Rust g5-api backend.
//
// SOURCE OF TRUTH: g5-api-routes/src/routes/*.rs  (Serialize DTOs with serde)
// All Rust DTOs use #[serde(rename_all = "camelCase")].
//
// These schemas serve two purposes:
// 1. Contract tests: validate that TypeScript interfaces match the API
// 2. Runtime validation: optionally validate API responses at runtime
// =============================================================================

import { z } from "zod";

export const AgentProviderSchema = z.enum(["claude_code", "codex"]);

// ---------------------------------------------------------------------------
// Response Envelope (mod.rs)
// ---------------------------------------------------------------------------

export const PaginationMetaSchema = z.object({
  page: z.number(),
  perPage: z.number(),
  total: z.number(),
});

/** Every API response is wrapped in { data: T, meta?: PaginationMeta } */
export function envelopeSchema<T extends z.ZodTypeAny>(dataSchema: T) {
  return z.object({
    data: dataSchema,
    meta: PaginationMetaSchema.optional(),
  });
}

// ---------------------------------------------------------------------------
// Auth DTOs (auth.rs)
// ---------------------------------------------------------------------------

export const OAuthRedirectDTOSchema = z.object({
  url: z.string(),
  state: z.string(),
});

export const AuthTokensDTOSchema = z.object({
  jwt: z.string(),
  // Omitted when the exchange used the httpOnly refresh cookie: the
  // browser keeps that credential in its cookie jar, never in script.
  refreshToken: z.string().optional(),
  expiresIn: z.number(),
  userId: z.string(),
});

export const ScopeCheckResponseSchema = z.object({
  allowed: z.boolean(),
  missing: z.array(z.string()),
});

export const ScmConnectionResultDTOSchema = z.object({
  provider: z.string(),
  providerUsername: z.string(),
  scopes: z.array(z.string()),
  connectedAt: z.string(),
  returnTo: z.string(),
});

export const ScmRepoDTOSchema = z.object({
  providerRepoId: z.string(),
  fullName: z.string(),
  name: z.string(),
  owner: z.string(),
  defaultBranch: z.string(),
  isPrivate: z.boolean(),
  cloneUrl: z.string(),
  description: z.string().nullable().optional(),
  language: z.string().nullable().optional(),
  updatedAt: z.string(),
  alreadyImported: z.boolean(),
  importedRepoId: z.string().nullable().optional(),
});

export const ScmRepoListDTOSchema = z.object({
  repos: z.array(ScmRepoDTOSchema),
  nextCursor: z.string().nullable().optional(),
});

export const ImportedRepoDTOSchema = z.object({
  id: z.string(),
  orgId: z.string(),
  name: z.string(),
  slug: z.string(),
  gitUrl: z.string(),
  defaultBranch: z.string(),
  provider: z.string().nullable().optional(),
  providerRepoId: z.string().nullable().optional(),
  providerFullName: z.string().nullable().optional(),
  createdAt: z.string(),
});

export const ImportReposResponseSchema = z.object({
  imported: z.array(ImportedRepoDTOSchema),
});

export const ScmOrgDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  avatarUrl: z.string().nullable().optional(),
});

// ---------------------------------------------------------------------------
// User DTOs (users.rs / user_service.rs)
// ---------------------------------------------------------------------------

export const OrgMembershipSummarySchema = z.object({
  orgId: z.string(),
  orgName: z.string(),
  orgSlug: z.string(),
  role: z.string(),
  orgKind: z.enum(["tenant", "operator"]),
});

export const ScmConnectionSummarySchema = z.object({
  provider: z.string(),
  providerUsername: z.string(),
  scopes: z.array(z.string()),
  hasInstallation: z.boolean(),
  connectedAt: z.string(),
});

export const CurrentUserDTOSchema = z.object({
  id: z.string(),
  email: z.string(),
  displayName: z.string(),
  avatarUrl: z.string().nullable().optional(),
  oauthProvider: z.string().nullable().optional(),
  orgs: z.array(OrgMembershipSummarySchema),
  scmConnections: z.array(ScmConnectionSummarySchema),
  createdAt: z.string(),
  onboardingStep: z.number().int().nullable().optional(),
  onboardingSkipped: z.boolean().optional(),
});

export const PublicUserDTOSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  avatarUrl: z.string().nullable().optional(),
});

// ---------------------------------------------------------------------------
// Org DTOs (orgs.rs)
// ---------------------------------------------------------------------------

export const OrgListItemDTOSchema = z.object({
  orgId: z.string(),
  orgName: z.string(),
  orgSlug: z.string(),
  role: z.string(),
  createdAt: z.string(),
});

export const OrgDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  slug: z.string(),
  createdAt: z.string(),
});

export const OrgSummaryDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  slug: z.string(),
  createdAt: z.string(),
});

export const WorkspaceSummaryDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
});

export const RepoSummaryDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  gitUrl: z.string(),
});

export const PlatformOrgDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  slug: z.string(),
  createdAt: z.string(),
  memberCount: z.number().int(),
});

export const CreatePlatformOrgResponseDTOSchema = z.object({
  org: OrgSummaryDTOSchema,
  admin: z.object({
    userId: z.string(),
    email: z.string(),
    accountCreated: z.boolean(),
  }),
});

export const UserRefDTOSchema = z.object({
  userId: z.string(),
  displayName: z.string(),
});

/** orgs.rs `OrgAdminSettings` — GET/PUT /api/v1/orgs/:orgId/admin-settings */
export const OrgAdminSettingsDTOSchema = z
  .object({
    defaultIdleTimeoutSecs: z.number().int().min(60).max(86_400),
    maxRunningVms: z.number().int().min(0).max(100),
    maxRunningVmsPerOwner: z.number().int().min(0).max(100).optional(),
    maxWorkspacesPerOwner: z.number().int().min(0).max(1000).optional(),
    autoAddHosts: z.boolean().optional(),
    autoHostInstanceType: z.string().optional(),
    autoRetireIdleHosts: z.boolean().optional(),
    idleHostRetireSecs: z.number().int().min(900).max(86_400).optional(),
    minFreeSlots: z.number().int().min(0).max(100).optional(),
    automaticContinuationEnabled: z.boolean().optional(),
  })
  .strict();

export const OrgMemberResponseDTOSchema = z.object({
  deactivatedAt: z.string().nullable(),
  activityRevision: z.number().int().nonnegative(),
  shutdownPending: z.boolean(),
  userId: z.string(),
  email: z.string(),
  displayName: z.string(),
  avatarUrl: z.string().nullable().optional(),
  role: z.string(),
  joinedAt: z.string(),
});

export const OrgInvitationDTOSchema = z.object({
  id: z.string(),
  orgId: z.string(),
  email: z.string(),
  role: z.string(),
  invitedBy: UserRefDTOSchema,
  status: z.string(),
  expiresAt: z.string(),
  createdAt: z.string(),
});

export const InviteMemberResponseDTOSchema = z.object({
  membership: OrgMemberResponseDTOSchema.optional(),
  invitation: OrgInvitationDTOSchema.optional(),
});

export const RepoDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  slug: z.string(),
  gitUrl: z.string(),
  defaultBranch: z.string(),
  provider: z.string().nullable().optional(),
  providerFullName: z.string().nullable().optional(),
  createdAt: z.string(),
});

// ---------------------------------------------------------------------------
// Workspace DTOs (workspaces.rs)
// ---------------------------------------------------------------------------

export const BranchSummaryDTOSchema = z.object({
  workspaceId: z.string(),
  repoId: z.string(),
  repoName: z.string(),
  branchName: z.string(),
  trackingMode: z.string(),
  headCommit: z.string(),
  // Nullable: `null` = unknown divergence (workspace_service ontology D50,
  // issue #1995). Only the workspace-detail path computes real values.
  divergence: z.number().nullable(),
  behindBase: z.number().nullable(),
  hasDraft: z.boolean(),
  dirtyFileCount: z.number(),
  // Present only for `trackingMode === "remote_tracking"` (skip_serializing_if
  // on the Rust DTO). Surface the remote identity on the ReposBar chip and the
  // workspace-selector dropdown.
  remoteBranch: z.string().optional(),
  remoteRepoName: z.string().optional(),
  // Option 1 (G5 #1198): authoritative per-child passive auto-pull setting.
  // Present only for `trackingMode === "remote_tracking"` children.
  autoPullEnabled: z.boolean().optional(),
  // Option 2 (G5 #1198): publish eligibility for a local-only branch (its fork
  // parent had this repo remote-tracked).
  remoteEligible: z.boolean().optional(),
});

export const WorkspaceKindSchema = z.enum(["base", "working", "personal"]);

export const WorkspaceListItemDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable().optional(),
  // Registry kind (g5.personal.identity); absent on older servers.
  kind: WorkspaceKindSchema.optional(),
  readOnly: z.boolean().optional().default(false),
  isReview: z.boolean().optional().default(false),
  // Workbench phase (g5.workbench.phases.state); absent on older servers.
  phase: z.enum(["be", "se", "ie"]).optional(),
  ownerUserId: z.string().nullable().optional(),
  clonedFromId: z.string().nullable().optional(),
  branches: z.array(BranchSummaryDTOSchema),
  tasksCompleted: z.number(),
  totalTasks: z.number(),
  runningTime: z.string().nullable().optional(),
  cost: z.number(),
  concurrentTasks: z.number(),
  lastUpdated: z.string(),
  createdAt: z.string(),
});

export const WorkspaceDetailDTOSchema = z.object({
  id: z.string(),
  orgId: z.string(),
  name: z.string(),
  isBase: z.boolean(),
  // Registry kind (g5.personal.identity); absent on older servers.
  kind: WorkspaceKindSchema.optional(),
  readOnly: z.boolean().optional().default(false),
  description: z.string().nullable().optional(),
  clonedFromId: z.string().nullable().optional(),
  docsRepoId: z.string().nullable().optional(),
  createdBy: z.string().nullable().optional(),
  ownerUserId: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
  // Workbench phase (g5.workbench.phases.state): GET detail and phase updates.
  phase: z.enum(["be", "se", "ie"]).optional(),
  branches: z.array(BranchSummaryDTOSchema),
  docsFolderPath: z.string(),
});

export const WorkspaceStatsDTOSchema = z.object({
  totalCostUsd: z.number(),
  tasksCompleted: z.number(),
  totalTasks: z.number(),
  concurrentTasks: z.number(),
  tasksStopped: z.number(),
  byTask: z.array(
    z.object({
      taskId: z.string(),
      taskType: z.string(),
      taskName: z.string(),

      costUsd: z.number(),
    }),
  ),
  branches: z.array(
    z.object({
      workspaceId: z.string(),
      repoId: z.string(),
      repoName: z.string(),
      branchName: z.string(),
      // Nullable: the stats path always reports unknown (`null`), never a
      // fabricated 0 (workspace_service ontology D50).
      aheadOfBase: z.number().nullable(),
      behindBase: z.number().nullable(),
    }),
  ),
});

export const WorkspaceBranchDetailDTOSchema = z.object({
  workspaceId: z.string(),
  repoId: z.string(),
  repoName: z.string(),
  branchName: z.string(),
  headCommit: z.string(),
  trackingMode: z.string(),
  hasDraft: z.boolean(),
  dirtyFileCount: z.number(),
  dirtyFiles: z.array(z.string()),
});

export const CoordinatedRepoEvidenceDTOSchema = z.object({
  workspaceId: z.string().optional(),
  repoId: z.string(),
  repoName: z.string(),
  branchName: z.string(),
  remoteBranch: z.string().optional(),
  operation: z.string(),
  status: z.string(),
  oldCommit: z.string().nullable().optional(),
  newCommit: z.string().nullable().optional(),
  error: z.string().optional(),
});

export const CoordinatedOperationResultDTOSchema = z.object({
  operation: z.string(),
  status: z.string(),
  message: z.string().optional(),
  repos: z.array(CoordinatedRepoEvidenceDTOSchema),
});

export const ImportHoldSchema = z.object({
  pullRequestId: z.string(),
  title: z.string(),
});

export const ForkSourceAdvisorySchema = z.object({
  repoId: z.string(),
  repoName: z.string(),
  kind: z.enum(["import_held", "paused", "behind"]),
  pullRequest: ImportHoldSchema.optional(),
  pauseReason: z.string().optional(),
  providerTip: z.string().optional(),
});

export const SyncResultDTOSchema = z.object({
  branchesUpdated: z.number(),
  branchesSkipped: z.number(),
  details: z.array(
    z.object({
      workspaceId: z.string(),
      repoId: z.string(),
      repoName: z.string(),
      branchName: z.string(),
      status: z.string(),
      oldCommit: z.string().nullable().optional(),
      newCommit: z.string().nullable().optional(),
      heldBy: ImportHoldSchema.optional(),
    }),
  ),
  coordinated: CoordinatedOperationResultDTOSchema.optional(),
});

export const PromoteRemoteTrackingResponseDTOSchema = z.object({
  branch: BranchSummaryDTOSchema,
  coordinated: CoordinatedOperationResultDTOSchema,
});

export const PushToRemoteResponseDTOSchema = z.object({
  enqueued: z.number(),
  coordinated: CoordinatedOperationResultDTOSchema.optional(),
});

export const MergeResultDTOSchema = z.object({
  branchesMerged: z.number(),
  branchesFailed: z.number(),
  details: z.array(
    z.object({
      workspaceId: z.string(),
      repoId: z.string(),
      repoName: z.string(),
      branchName: z.string(),
      status: z.string(),
      mergeCommit: z.string().nullable().optional(),
      error: z.string().nullable().optional(),
    }),
  ),
});

export const CloneWorkspaceResponseDTOSchema = z.object({
  workspace: WorkspaceListItemDTOSchema,
  clonedBranches: z.number(),
  clonedDrafts: z.number(),
  carriedSessions: z.number(),
  sourceAdvisories: z.array(ForkSourceAdvisorySchema).optional(),
});

export const WorkspaceMergeRunSummarySchema = z.object({
  id: z.string(),
  workspaceId: z.string(),
  sourceKind: z.string().optional(),

  sourceWorkspaceId: z.string().nullable().optional(),
  targetWorkspaceId: z.string().nullable().optional(),

  sourceBranchName: z.string(),
  status: z.string(),
  phase: z.string(),
  blockingReason: z.string().nullable().optional(),
  repoId: z.string().optional(),
  sourceHeads: z
    .object({
      workspace_tip: z.string().optional(),
      mirror_tip: z.string().optional(),
    })
    .passthrough()
    .optional(),
  targetHeads: z
    .object({ baseline: z.string().optional() })
    .passthrough()
    .optional(),
  startedAt: z.string().optional(),
});

export const ListBranchMergeRunsResponseSchema = z.object({
  items: z.array(WorkspaceMergeRunSummarySchema),
});

export const CompleteBranchMergeRunResponseSchema = z.object({
  completed: z.boolean(),
});

export const StartBranchMergeRunResponseSchema = z.object({
  run: WorkspaceMergeRunSummarySchema,
});

export const StartWorkspaceMergeResponseSchema = z.union([
  z.object({ run: WorkspaceMergeRunSummarySchema }),
  z.object({
    resolution: z.object({
      taskId: z.string(),
      sessionId: z.string(),
      status: z.string(),
    }),
    startCompute: z
      .object({ intentId: z.string(), status: z.string() })
      .optional(),
  }),
  z.object({
    rebase: z.object({
      sourceWorkspaceId: z.string(),
      targetWorkspaceId: z.string(),
      vmId: z.string(),
      members: z.array(
        z.object({
          repoId: z.string(),
          previousHead: z.string(),
          head: z.string(),
          published: z.boolean(),
        }),
      ),
    }),
  }),
]);

// ---------------------------------------------------------------------------
// Task DTOs (tasks.rs)
// ---------------------------------------------------------------------------

export const SpecContextDTOSchema = z.object({
  componentId: z.string(),
  srcFiles: z.array(z.string()),
  parentSpec: z.string().nullable().optional(),
  childSpecs: z.array(z.string()),
});

export const TaskDTOSchema = z.object({
  id: z.string(),

  name: z.string(),
  taskType: z.string(),
  status: z.enum([
    "queued",
    "running",
    "waiting",
    "succeeded",
    "failed",
    "cancelled",
  ]),
  workspaceId: z.string(),
  workspaceName: z.string(),

  specContext: SpecContextDTOSchema.nullable().optional(),
  dependsOn: z.array(z.string()),
  filesChanged: z.array(z.string()),
  costUsd: z.number(),
  sessionId: z.string().nullable().optional(),
  assignedAt: z.string().nullable().optional(),
  completedAt: z.string().nullable().optional(),
  lifecyclePosture: z
    .enum(["active", "resumable", "action_required", "terminal"])
    .optional(),
  latestAttempt: z
    .object({
      id: z.string(),
      attemptNumber: z.number(),
      provider: z.string().nullable(),
      providerSessionId: z.string().nullable(),
      disposition: z.string().nullable(),
      reason: z.string().nullable(),
      retryPolicy: z.string().nullable(),
      lifecyclePosture: z.enum([
        "active",
        "resumable",
        "action_required",
        "terminal",
      ]),
      startedAt: z.string(),
      endedAt: z.string().nullable(),
      continuationSummary: z.string().nullable().optional(),
    })
    .nullable()
    .optional(),
});

// `GET /api/v1/sessions/:id/task` (routes/chat I19, issue #1597 review):
// the session's newest live task, or null when the session has no live
// task — a normal state, not an error.
export const WorkspaceTaskPageSchema = z.object({
  tasks: z.array(
    TaskDTOSchema.pick({
      id: true,
      name: true,
      taskType: true,
      status: true,
      workspaceId: true,
      sessionId: true,
      costUsd: true,
    }).extend({ createdAt: z.string(), updatedAt: z.string() }),
  ),
  cursor: z.string().nullable(),
  hasMore: z.boolean(),
});

export const SessionTaskResponseSchema = TaskDTOSchema.nullable();

export const TimelineEntryDTOSchema = z.object({
  id: z.string(),
  entryType: z.string(),
  content: z.string(),
  metadata: z.unknown(), // serde_json::Value
  createdAt: z.string(),
});

export const TimelineResponseSchema = z.object({
  entries: z.array(TimelineEntryDTOSchema),
  cursor: z.string().nullable().optional(),
  hasMore: z.boolean(),
});

export const LiveTailResponseDTOSchema = z.object({
  state: z.enum(["none", "live", "catching_up"]),
  taskId: z.string().nullable().optional(),
  attempt: z.number().int().nullable().optional(),
  persistedThrough: z.number().int().nullable().optional(),
  fetchedThrough: z.number().int().nullable().optional(),
  entries: z.array(TimelineEntryDTOSchema),
});

export const DiffLineSchema = z.object({
  type: z.string(), // serde rename from line_type
  oldLine: z.number().nullable().optional(),
  newLine: z.number().nullable().optional(),
  content: z.string(),
});

export const DiffHunkSchema = z.object({
  startLine: z.number(),
  header: z.string(),
  lines: z.array(DiffLineSchema),
});

export const TaskFileDiffSchema = z.object({
  // A task diff spans every repo the task touches — each entry is
  // stamped with the repo it was computed in so file paths stay unambiguous.
  repoId: z.string(),
  repoName: z.string(),
  file: z.string(),
  additions: z.number(),
  deletions: z.number(),
  hunks: z.array(DiffHunkSchema),
});

// ---------------------------------------------------------------------------
// Comment DTOs (comments.rs)
// ---------------------------------------------------------------------------

export const CommentDTOSchema = z.object({
  id: z.string(),
  workspaceId: z.string(),
  authorId: z.string(),
  authorName: z.string(),
  authorAvatarUrl: z.string().nullable().optional(),
  scope: z.string(),
  repoId: z.string().nullable(),
  filePath: z.string().nullable().optional(),
  lineNumber: z.number().nullable().optional(),
  commitSha: z.string().nullable().optional(),
  content: z.string(),
  resolved: z.boolean(),
  resolvedAt: z.string().nullable().optional(),
  originBranch: z.string(),
  replyCount: z.number(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const ReplyDTOSchema = z.object({
  id: z.string(),
  commentId: z.string(),
  authorId: z.string(),
  authorName: z.string(),
  authorAvatarUrl: z.string().nullable().optional(),
  content: z.string(),
  createdAt: z.string(),
});

export const UserSummaryDTOSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  avatarUrl: z.string().nullable().optional(),
});

export const CrossBranchCommentItemSchema = z.object({
  id: z.string(),
  repoId: z.string(),
  originBranch: z.string(),
  author: UserSummaryDTOSchema,
  content: z.string(),
  lineNumber: z.number(),
  commitSha: z.string(),
  mappedLine: z.number().nullable().optional(),
  createdAt: z.string(),
});

export const CrossBranchCommentResponseSchema = z.object({
  comments: z.array(CrossBranchCommentItemSchema),
  branchCount: z.number(),
  totalCount: z.number(),
});

// ---------------------------------------------------------------------------
// Chat DTOs (chat.rs)
// ---------------------------------------------------------------------------

export const SessionListItemDTOSchema = z.object({
  id: z.string(),
  workspaceId: z.string(),
  scopeType: z.string(),
  scopeRef: z.string().nullable().optional(),
  model: z.string().nullable().optional(),
  agentProvider: AgentProviderSchema.default("claude_code"),
  providerState: z.record(z.string(), z.unknown()).nullable().optional(),
  title: z.string().nullable().optional(),
  summary: z.string().nullable().optional(),
  tags: z.array(z.string()),
  messageCount: z.number(),
  totalInputTokens: z.number(),
  totalOutputTokens: z.number(),
  totalCostUsd: z.number(),
  forkedFromId: z.string().nullable().optional(),
  linkedCommits: z.array(z.string()),
  createdAt: z.string(),
  updatedAt: z.string(),
  canSend: z.boolean(),
});

/**
 * Historical quota responses remain readable. Current chat meters arrive
 * through the live channel; session reads omit subscriptionUsage.
 */
export const SubscriptionUsageSchema = z.object({
  fiveHourUtilization: z.number().nullable().optional(),
  fiveHourResetAt: z.string().nullable().optional(),
  sevenDayUtilization: z.number().nullable().optional(),
  sevenDayResetAt: z.string().nullable().optional(),
  representativeClaim: z.string().nullable().optional(),
  unifiedStatus: z.string().nullable().optional(),
  capturedAt: z.string(),
});

export const SessionDetailDTOSchema = SessionListItemDTOSchema.extend({
  forkedAtIndex: z.number().nullable().optional(),
  // Additive-optional so old payloads without a persisted launch / quota
  // snapshot still parse (agent_interaction_ux D8;
  // external_agent_provider_contract D11).
  authMode: z.string().nullable().optional(),
  subscriptionUsage: SubscriptionUsageSchema.nullable().optional(),
});

export const ChatMessageDTOSchema = z.object({
  id: z.string(),
  sessionId: z.string(),
  role: z.string(),
  content: z.string(),
  artifacts: z.unknown().nullable().optional(), // serde_json::Value
  citations: z.unknown(), // serde_json::Value
  inputTokens: z.number().nullable().optional(),
  outputTokens: z.number().nullable().optional(),
  status: z.string(),
  messageIndex: z.number(),
  createdAt: z.string(),
  isErrorNotice: z.boolean().optional(),
  codexAuthFailure: z
    .object({ taskId: z.string(), attempt: z.number().int() })
    .optional(),
});

export const MessageListResponseDTOSchema = z.object({
  messages: z.array(ChatMessageDTOSchema),
  cursor: z.number().nullable().optional(),
  hasMore: z.boolean(),
});

export const SendMessageResponseDTOSchema = z.object({
  messageId: z.string(),
  status: z.string(),
  createdAt: z.string().optional(),
  inputId: z.string().optional(),
  taskId: z.string().optional(),
  delivery: z.string().optional(),
});

export const StopGenerationResponseDTOSchema = z.object({
  stopped: z.boolean(),
  disposition: z.string().optional(),
});

export const SessionInputDTOSchema = z.object({
  id: z.string(),
  sessionId: z.string(),
  inputIndex: z.number(),
  content: z.string(),
  artifacts: z.unknown(),
  revision: z.number(),
  inputOrigin: z.enum(["human", "workflow", "continuation"]),
  delivery: z.enum([
    "queued",
    "reserved",
    "delivered",
    "delivery_unknown",
    "withdrawn",
  ]),
  deliveryGeneration: z.number(),
  taskId: z.string().nullable().optional(),
  attempt: z.number().nullable().optional(),
  createdAt: z.string(),
  deliveredAt: z.string().nullable().optional(),
  withdrawnAt: z.string().nullable().optional(),
  editable: z.boolean().optional(),
  withdrawable: z.boolean().optional(),
});

export const StartSessionResponseDTOSchema = z.object({
  session: SessionDetailDTOSchema,
  message: SendMessageResponseDTOSchema,
  assistantMessageId: z.string(),
  taskId: z.string(),
});

export const ForkSessionResponseDTOSchema = z.object({
  sessionId: z.string(),
  messagesCopied: z.number(),
});

export const AttachSessionResponseDTOSchema = z.object({
  sessionId: z.string(),
  taskId: z.string().nullable().optional(),
  wsChannel: z.string(),
});

export const DocListItemDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  docType: z.string(),
  filePath: z.string().nullable().optional(),
  documentSource: z
    .enum(["user", "auxiliary"])
    .nullable()
    .optional()
    .catch(null),
  linkedNodeCount: z.number(),
  lastUpliftedAt: z.string().nullable().optional(),
  lastDownliftedAt: z.string().nullable().optional(),
  contentUpdatedAt: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const DocDetailDTOSchema = z.object({
  id: z.string(),
  workspaceId: z.string(),
  name: z.string(),
  docType: z.string(),
  filePath: z.string().nullable().optional(),
  documentSource: z
    .enum(["user", "auxiliary"])
    .nullable()
    .optional()
    .catch(null),
  content: z.string().nullable().optional(),
  contentHash: z.string().nullable().optional(),
  source: z.string().optional(),
  repoOid: z.string().nullable().optional(),
  snapshotAt: z.string().nullable().optional(),
  lastUpliftedAt: z.string().nullable().optional(),
  lastDownliftedAt: z.string().nullable().optional(),
  contentUpdatedAt: z.string().nullable().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

// ---------------------------------------------------------------------------
// File DTOs (files.rs)
// ---------------------------------------------------------------------------

export const FileTreeEntrySchema: z.ZodType<{
  name: string;
  path: string;
  type: string;
  size: number | null;
  children?: Array<{
    name: string;
    path: string;
    type: string;
    size: number | null;
    children?: unknown[];
  }>;
}> = z.object({
  name: z.string(),
  path: z.string(),
  type: z.string(), // serde rename from entry_type
  size: z.number().nullable(),
  children: z.lazy(() => z.array(FileTreeEntrySchema)).optional(),
});

export const FileContentResponseSchema = z.object({
  path: z.string(),
  content: z.string(),
  size: z.number(),
  encoding: z.string(),
  draftDirty: z.boolean(),
});

// ---------------------------------------------------------------------------
// Instance DTOs (instance.rs)
// ---------------------------------------------------------------------------

export const InstanceInfoDTOSchema = z.object({
  version: z.string(),
  features: z.array(z.string()),
  authProviders: z.array(z.string()),
  maxUploadSizeMb: z.number(),
  selfHosted: z.boolean(),
  devLoginEnabled: z.boolean(),
  motd: z.string().nullable().optional(),
});

export const HealthCheckResponseSchema = z.object({
  status: z.string(),
  checks: z.object({
    database: z.string(),
  }),
  uptime: z.number(),
});

export const ChatModelDTOSchema = z.object({
  id: z.string(),
  name: z.string(),
  provider: z.string(),
  contextWindow: z.number().nullable(),
  maxOutputTokens: z.number().nullable(),
  supportsStreaming: z.boolean(),
  costPer1kInput: z.number().nullable(),
  costPer1kOutput: z.number().nullable(),
  agentProvider: z.enum(["claude_code", "codex"]),
  route: z.enum(["direct", "openrouter"]),
  efforts: z.array(z.string()),
  defaultEffort: z.string(),
  modes: z.array(z.string()),
  cliVersion: z.string(),
});

// ---------------------------------------------------------------------------
// Settings DTOs (settings.rs)
// ---------------------------------------------------------------------------

export const WorkspaceSettingsDTOSchema = z.object({
  workspaceId: z.string(),
  maxSpendUsd: z.number().nullable().optional(),
  spendPeriod: z.string(),
  spendPeriodStart: z.string().nullable().optional(),
  maxInputTokens: z.number().nullable().optional(),
  maxOutputTokens: z.number().nullable().optional(),
  idleTimeoutSecs: z.number(),
  responseDeadlineSecs: z.number(),
  boundedExecutionSlots: z.number(),
  permissionPolicy: z.record(z.string(), z.unknown()).nullable().optional(),
  agentMayModifyOtherWorkspaces: z.boolean(),
  updatedBy: z.string().nullable().optional(),
  updatedAt: z.string(),
});

export const TaskSpendBreakdownDTOSchema = z.object({
  taskId: z.string(),
  taskType: z.string(),

  taskName: z.string(),

  spendUsd: z.number(),
  inputTokens: z.number(),
  outputTokens: z.number(),
});

export const SpendSummaryRouteDTOSchema = z.object({
  periodStart: z.string(),
  periodEnd: z.string(),
  totalSpendUsd: z.number(),
  totalInputTokens: z.number(),
  totalOutputTokens: z.number(),
  limitSpendUsd: z.number().nullable().optional(),
  limitInputTokens: z.number().nullable().optional(),
  limitOutputTokens: z.number().nullable().optional(),
  isSpendLimitReached: z.boolean(),
  byTask: z.array(TaskSpendBreakdownDTOSchema),
  unattributedSpendUsd: z.number(),
  otherTaskSpendUsd: z.number(),
});

export const SpendPeriodRouteDTOSchema = z.object({
  periodStart: z.string(),
  periodEnd: z.string(),
  totalSpendUsd: z.number(),
  totalInputTokens: z.number(),
  totalOutputTokens: z.number(),

  taskCount: z.number(),
});

export const DailySpendRouteDTOSchema = z.object({
  date: z.string(),
  spendUsd: z.number(),
  inputTokens: z.number(),
  outputTokens: z.number(),
});

export const SpendHistoryRouteDTOSchema = z.object({
  granularity: z.string(),
  periods: z.array(SpendPeriodRouteDTOSchema),
  daily: z.array(DailySpendRouteDTOSchema),
});

// ---------------------------------------------------------------------------
// Internal DTOs (internal.rs)
// ---------------------------------------------------------------------------

export const WorkerTerminateResponseSchema = z.object({
  acknowledged: z.boolean(),
});

export const UsageAcknowledgementSchema = z.object({
  inserted: z.number(),
});

// ---------------------------------------------------------------------------
// Permissions DTOs (permissions.rs)
// ---------------------------------------------------------------------------

export const EffectiveScopesResponseSchema = z.object({
  scopes: z.array(z.string()),
  orgDefaults: z.array(z.string()),
  userGrants: z.array(z.string()),
  workspaceOverrides: z.array(z.string()),
});

// ---------------------------------------------------------------------------
// Resolve DTO (resolve.rs)
// ---------------------------------------------------------------------------

export const ResolveDTOSchema = z.object({
  orgId: z.string(),
  repoId: z.string().nullable().optional(),
  workspaceId: z.string().nullable().optional(),
});

// ---------------------------------------------------------------------------
// Remote Repo Tracking — sync runs panel (G1)
// ---------------------------------------------------------------------------

export const BranchSyncRunSchema = z.object({
  id: z.string(),
  workspaceId: z.string(),
  repoId: z.string(),
  actingUserId: z.string(),
  kind: z.string(),
  triggerKind: z.string(),
  status: z.string(),
  oldWorkspaceTip: z.string().optional(),
  newWorkspaceTip: z.string().optional(),
  oldMirrorTip: z.string().optional(),
  newMirrorTip: z.string().optional(),
  newBaseline: z.string().optional(),
  errorDetail: z.string().optional(),
  mergeRunId: z.string().optional(),
  startedAt: z.string(),
  completedAt: z.string().optional(),
});

export const BranchTrackingRepoSchema = z.object({
  repoId: z.string(),
  repoName: z.string(),
  repoBranchName: z.string(),
  trackingMode: z.string(),
  headCommit: z.string(),
  remoteBranch: z.string().optional(),
  baselineCommit: z.string().optional(),
  syncPausedReason: z.string().optional(),
  lastSyncAt: z.string().optional(),
  remoteTipAdvisorySha: z.string().optional(),
  remoteTipAdvisoryObservedAt: z.string().optional(),
  latestRunStatus: z.string().optional(),
  importHeldBy: ImportHoldSchema.optional(),
  rewritePending: z.boolean().optional(),
});

export const BranchTrackingSummarySchema = z.object({
  workspaceId: z.string(),
  repos: z.array(BranchTrackingRepoSchema),
});

export const ListBranchSyncRunsResponseSchema = z.object({
  branch: BranchTrackingSummarySchema,
  runs: z.object({
    data: z.array(BranchSyncRunSchema),
    total: z.number(),
    page: z.number(),
    perPage: z.number(),
  }),
});

export const RetryBranchSyncRunsResponseSchema = z.object({
  enqueued: z.number(),
  outcome: z.string(),
});
