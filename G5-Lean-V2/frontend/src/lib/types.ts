// =============================================================================
// G5 Client Data Layer — Shared TypeScript Types
// =============================================================================
// This file is the single source of truth for all TypeScript interfaces, type
// aliases, and enumerations used across the G5 client frontend.
//
// RULES:
// - Only TypeScript type declarations (interface, type, enum)
// - No runtime values (no classes, functions, constants)
// - All field names use camelCase (matching g5-api JSON serialization)
// - Named exports only (no default export)
// =============================================================================

// ---------------------------------------------------------------------------
// 1. Universal Primitives
// ---------------------------------------------------------------------------

/** Universal status type used for dots, tints, and labels across all components */
export type State = "active" | "ok" | "info" | "warn" | "blocked" | "muted";

/** 12 named themes × 2 modes (dark/light) */
export type ThemeName =
  | "editor"
  | "signal-studio"
  | "obsidian-gold"
  | "carbon-clay"
  | "alpine-graphite"
  | "darkroom"
  | "ink-parchment"
  | "studio-night"
  | "pm-paper"
  | "aero-glass"
  | "classic-98"
  | "green-room";

/** Common "loading / error / data" wrapper for hook consumers */
export type AsyncState<T> =
  | { status: "loading" }
  | { status: "error"; error: Error }
  | { status: "ok"; data: T };

/** Pagination envelope returned by list endpoints */
export interface Page<T> {
  items: T[];
  cursor: string | null; // opaque; pass to next request
  hasMore: boolean;
}

/** Wrapper for all paginated API responses (alternative name used by some endpoints) */
export interface PaginatedResponse<T> {
  data: T[];
  cursor: string | null;
  hasMore: boolean;
  total?: number; // Only when cheaply computable
}

/** WebSocket event pushed to the client */
export interface WSEvent<T = unknown> {
  channel: string; // e.g. "project:uuid", "task:uuid"
  event: string; // e.g. "status_changed", "timeline_append"
  payload: T;
}

/** Uniform API error shape (matches backend §4B error catalog) */
export interface ApiError {
  code: string; // Machine-readable (e.g., 'FORBIDDEN')
  message: string; // Human-readable
  details?: Record<string, unknown>;
  requestId: string;
}

/** Standard mutation return type for React Query mutation hooks */
export interface MutationHook<TInput, TOutput> {
  mutate: (input: TInput) => void;
  mutateAsync: (input: TInput) => Promise<TOutput>;
  isLoading: boolean;
  error: Error | null;
  data: TOutput | undefined;
  reset: () => void;
}

// ---------------------------------------------------------------------------
// 2. User & Auth Types
// ---------------------------------------------------------------------------

/** Lightweight user ref embedded in DTOs */
export interface UserRef {
  id: string;
  displayName: string;
  avatarUrl: string | null;
  initial: string; // first letter, for avatar fallback
  color: string; // g5 token name for avatar tint
}

/** User profile — personal stats */
export interface UserProfile {
  id: string;
  name: string;
  email: string;
  avatar: string;
  usage: {
    totalCost: number;
    totalTokens: number;
    totalTasks: number;
  };
}

/** Organization kind; at most one `operator` organization exists. */
export type OrgKind = "tenant" | "operator";

/** Current authenticated user DTO from GET /api/v1/auth/me */
export interface CurrentUserDTO {
  id: string;
  email: string;
  displayName: string;
  avatarUrl: string | null;
  oauthProvider: string | null; // nullable — not all users have OAuth
  /** The org ID from the current JWT session context, if any. */
  activeOrgId: string | null;
  orgs: Array<{
    orgId: string;
    orgName: string;
    orgSlug: string;
    role: string; // 'admin' | 'member' | 'viewer'
    /** `operator` only for the platform operator organization (g5.organization.platform-operator.identity). */
    orgKind: OrgKind;
  }>;
  scmConnections: Array<{
    provider: string; // 'github' | 'gitlab' | 'bitbucket' etc.
    providerUsername: string;
    scopes: string[];
    hasInstallation: boolean;
    connectedAt: string; // ISO 8601
  }>;
  createdAt: string;
  /**
   * Highest onboarding-tour step reached (users g5.user.onboarding): null
   * until the welcome card is shown, 7 when finished. Older servers omit it.
   */
  onboardingStep?: number | null;
  /** Whether the person skipped the tour at `onboardingStep`. */
  onboardingSkipped?: boolean;
  /**
   * Account-wide "Approval requests" email preference, for both approval emails
   * (users g5.user.notifications). Default true; older servers omit it.
   */
  emailOnApprovalRequest?: boolean;
  /** Personal default across owned workspaces; explicit PR choices take priority. */
  stopVmAfterMergeDefault?: boolean;
}

/** A repository visible through an SCM provider listing */
export interface ScmRepoDTO {
  providerRepoId: string;
  fullName: string;
  name: string;
  owner: string;
  defaultBranch: string;
  isPrivate: boolean;
  cloneUrl: string;
  description: string | null;
  language: string | null;
  updatedAt: string;
  alreadyImported: boolean;
  importedRepoId: string | null;
}

/** Response from GET /api/v1/scm/:provider/repos */
export interface ScmRepoListDTO {
  repos: ScmRepoDTO[];
  nextCursor: string | null;
}

/** Repository created via SCM import */
export interface ImportedRepoDTO {
  id: string;
  orgId: string;
  name: string;
  slug: string;
  gitUrl: string;
  defaultBranch: string;
  provider: string | null;
  providerRepoId: string | null;
  providerFullName: string | null;
  createdAt: string;
}

/** Response from POST /api/v1/scm/:provider/repos/import */
export interface ImportReposResponse {
  imported: ImportedRepoDTO[];
}

// ---------------------------------------------------------------------------
// 3. Organization Types
// ---------------------------------------------------------------------------

/** Organization — billing + usage (mock/display model) */
export interface Organization {
  id: string;
  name: string;
  plan: "enterprise" | "team" | "free";
  role: "admin" | "member" | "viewer";
  billing: {
    currentSpend: number;
    budget: number;
    period: string;
  };
  usage: {
    inputTokens: number;
    outputTokens: number;
    tasksRun: number;
    agentHours: number;
  };
}

/** Org member DTO from GET /api/v1/orgs/:orgId/members */
/**
 * Organization settings edited on the Admin page
 * (`GET/PUT /api/v1/orgs/:orgId/admin-settings`, administrators only).
 * Values are effective: the platform default when no administrator set one.
 */
export interface OrgAdminSettingsDTO {
  hostDiskAllowanceGib?: number;
  vmDiskAllowanceGib?: number;
  /** Idle timeout every new workspace starts with (60–86400 s). */
  defaultIdleTimeoutSecs: number;
  /**
   * Idle timeout every new personal workspace starts with (60–86400 s,
   * initially 1800; g5.personal.capacity). Optional until the API reports it.
   */
  defaultPersonalIdleTimeoutSecs?: number;
  /** Most starting/running/stopping workspace VMs the org may hold (0–100). */
  maxRunningVms: number;
  /**
   * Most VMs one owner may hold starting, running, stopping or waiting to
   * start (0–100). Optional until the API reports it.
   */
  maxRunningVmsPerOwner?: number;
  /**
   * Most live workspaces, pending forks and transfers one owner may hold
   * (0–1000). Optional until the API reports it.
   */
  maxWorkspacesPerOwner?: number;
  // Managed capacity (Admin page group). Optional until the API reports them;
  // the card falls back to today's behavior when absent.
  /** Add cloud hosts for workspace starts waiting on capacity. */
  autoAddHosts?: boolean;
  /** Machine type for automatically added hosts (WORKER_HOST_INSTANCE_TYPES). */
  autoHostInstanceType?: string;
  /** Remove cloud hosts that stay empty. */
  autoRetireIdleHosts?: boolean;
  /** How long a host must stay empty before removal (900–86400 s). */
  idleHostRetireSecs?: number;
  /**
   * Free current slots to keep ready in the organization's region (0–100);
   * removal never goes below it. 0 turns the floor off.
   */
  minFreeSlots?: number;
  automaticContinuationEnabled?: boolean;
}

export interface OrgMemberDTO {
  deactivatedAt: string | null;
  activityRevision: number;
  shutdownPending: boolean;
  userId: string;
  email: string;
  displayName: string;
  avatarUrl: string | null;
  role: "admin" | "member" | "viewer";
  joinedAt: string;
}

/** Org invitation DTO */
export interface OrgInvitationDTO {
  id: string;
  orgId: string;
  email: string;
  role: "admin" | "member" | "viewer";
  invitedBy: UserRef;
  status: "pending" | "accepted" | "declined" | "expired";
  expiresAt: string; // 7 days from creation
  createdAt: string;
  /** Latest invitation email, listed for administrators; absent when none was requested. */
  emailDelivery?: InvitationEmailDeliveryDTO;
}

/** The latest email G5 requested for an invitation. */
export interface InvitationEmailDeliveryDTO {
  status: "queued" | "sending" | "sent" | "failed" | "cancelled";
  /** An attempt may have reached the provider without a definite answer. */
  outcomeUncertain: boolean;
  reason?: string;
  updatedAt: string;
}

/** Response from POST /api/v1/orgs/:orgId/members */
export interface InviteMemberResponseDTO {
  membership?: OrgMemberDTO;
  invitation?: OrgInvitationDTO;
}

/**
 * One human approver on an org approval definition, profile-enriched.
 * `stillMember=false` marks a departed member who remains in the version's
 * approver set (historical fact — never silently dropped).
 */
export interface OrgApprovalHumanDTO {
  userId: string;
  displayName: string;
  email: string;
  avatarUrl: string | null;
  stillMember: boolean;
  deactivatedAt?: string | null;
}

/**
 * An org approval definition at its latest version
 * (GET /api/v1/orgs/:orgId/approval-definitions). `firstCreatedBy`/
 * `firstCreatedAt` are v1 provenance — the library card's "Created by" line;
 * `createdBy`/`createdAt` describe the latest version.
 */
export interface OrgApprovalDefinitionDTO {
  id: string;
  latestVersion: number;
  name: string;
  description: string | null;
  humans: OrgApprovalHumanDTO[];
  agentType: "simple" | null;
  agentPrompt: string | null;
  /** Exact immutable launch policy for an agent-backed definition version. */
  agentHarness?: PickerHarness | null;
  agentModel?: string | null;
  agentEffort?: string | null;
  invalidatesPrevious: boolean;
  /** Missing legacy payloads retain the old all-reviewer policy. */
  requireAllReviewers?: boolean;
  humanApprovalsOverrideAgent?: boolean;
  allowSelfApproval?: boolean;
  /**
   * Phase binding (g5.workbench.gates): the workbench phases this version
   * binds, deduplicated; empty means no recommendation. Optional here for
   * tolerant decoding of an old cached payload, where the server default
   * (`["ie"]`) applies. Resolve through `definitionPhases`
   * (lib/approvalPhases) rather than reading the raw field.
   */
  phases?: WorkspacePhase[];
  createdBy: { userId: string; displayName: string };
  createdAt: string;
  firstCreatedBy: { userId: string; displayName: string };
  firstCreatedAt: string;
}

/** Per-approver current state on an outgoing workspace approval requirement. */
export interface ApprovalApproverStateDTO {
  userId: string;
  displayName: string;
  email: string;
  stillMember: boolean;
  deactivatedAt?: string | null;
  state: "approved" | "rejected" | "pending" | "not_requested";
  comment: string | null;
  decidedAt: string | null;
  requestId?: string | null;
  requestState?: ApprovalRequestState | null;
  requestedAt?: string | null;
  closedAt?: string | null;
}

export type ApprovalRequestState =
  | "pending"
  | "settled"
  | "cancelled"
  | "expired"
  | "superseded"
  /** Closed when a merge with override that waived G5 requirements landed
   * (g5.governance.approval.request-status); never an approval. */
  | "bypassed";

export type AgentApprovalReviewState = TaskStatus;

/** Observational state of an ordinary review conversation; not an approval decision. */
export interface AgentApprovalReviewDTO {
  reviewId?: string | null;
  stage?: string | null;
  reportAvailable?: boolean;
  retryAvailable?: boolean;
  contentSha256?: string | null;
  cleanupPending?: boolean;
  cleanupOverdue?: boolean;
  cleanupError?: string | null;
  taskId: string | null;
  sessionId: string | null;
  state: AgentApprovalReviewState;
  terminalReason: string | null;
  createdAt: string;
  startedAt: string | null;
  endedAt?: string | null;
  updatedAt: string;
}

/** One durable approval actor-slot request. */
export interface ApprovalRequestDTO {
  note?: string | null;
  requestId: string;
  actorType: "human" | "agent";
  actorUserId: string | null;
  actor: { userId: string; displayName: string } | null;
  requestedBy: { userId: string; displayName: string } | null;
  state: ApprovalRequestState;
  definitionVersion: number;
  definitionName: string;
  /**
   * Phase of the request chain (g5.workbench.gates); `null` for a request
   * admitted under the stage model before the cutover (terminal since then).
   */
  phase?: WorkspacePhase | null;
  requestedAt: string;
  closedAt: string | null;
  agentReviews: AgentApprovalReviewDTO[];
}

export interface HumanApprovalRequestAdmissionDTO {
  requestId: string;
  actorUserId: string;
  status: ApprovalRequestState;
  reused: boolean;
}

export interface HumanApprovalRequestsAdmissionDTO {
  requests: HumanApprovalRequestAdmissionDTO[];
}

export interface AgentApprovalReviewAdmissionDTO {
  requestId: string;
  reviewId: string;
  status: ApprovalRequestState;
  reused: boolean;
}

/** One immutable decision-log entry (approval_statuses row). */
export interface ApprovalDecisionLogEntryDTO {
  agentReviewId?: string | null;
  requestId?: string | null;
  outcome: "approved" | "rejected" | "deferred";
  comment: string | null;
  /** Agent-authored summary (schema 5+); null for legacy agent reports. */
  summary?: string | null;
  actorType?: "human" | "agent";
  actor?: { userId: string; displayName: string } | null;
  definitionVersion: number;
  superseded: boolean;
  reviewedGitHeads: unknown;
  createdAt: string;
}

/** One requirement of the workspace's merge target, with decision state. */
export interface WorkspaceOutgoingRequirementDTO {
  definition: OrgApprovalDefinitionDTO;
  /**
   * The phase this entry projects (g5.workbench.gates). The server emits one
   * entry per definition; its phase is a compatibility field. Optional for tolerant decoding of an
   * old cached payload, where a missing value reads as `"ie"`. Resolve
   * through `requirementPhase` (lib/approvalPhases) rather than reading it
   * raw.
   */
  phase?: WorkspacePhase;
  /**
   * The requirement's one authoritative overall status
   * (g5.governance.approval.status). Consumers count and show it; they never
   * derive status from approver rows.
   */
  state: ApprovalRequirementStatus;
  approvers: ApprovalApproverStateDTO[];
  callerCanDecide: boolean;
  requests?: ApprovalRequestDTO[];
  decisionLog: ApprovalDecisionLogEntryDTO[];
  /** Aggregate commit currentness of counted evidence; never changes `state`. */
  currentness?: ApprovalRequirementCurrentnessDTO;
  /** Structured remaining path; null for untouched or approved requirements. */
  approvalPath?: ApprovalPathDTO | null;
  /** Server-resolved recipients the request dialog may preselect. */
  requestNeeds?: ApprovalRequestNeedsDTO;
  /**
   * How an overridable agent rejection reads (g5.governance.approval.status):
   * a display of counted evidence, never a status or count category.
   */
  agentEscalation?: ApprovalAgentEscalationDTO | null;
}

export interface ApprovalAgentEscalationDTO {
  state: "needsHumanEscalation" | "overriddenByHuman";
  /** The counted agent decision whose badge this display relabels. */
  agentReviewId: string | null;
}

export type ApprovalRequirementStatus = "approved" | "rejected" | "pending";

export interface ApprovalReviewerRefDTO {
  actorType: "human" | "agent";
  /** Null for the requirement's agent. */
  userId: string | null;
  displayName: string;
}

export interface ApprovalRequirementCurrentnessDTO {
  /** `none` when no counted approved/rejected evidence exists. */
  state: "matching" | "changed" | "unknown" | "none";
  /** Reviewers whose counted evidence differs from the current source. */
  notCurrent: ApprovalReviewerRefDTO[];
}

export type ApprovalPathDTO =
  | {
      kind: "humansRejected";
      humans: ApprovalReviewerRefDTO[];
      /** All-of with other configured humans who must also approve. */
      othersRequired: boolean;
    }
  | {
      kind: "agentRejected";
      agentName: string;
      /** A configured human approval may override the agent rejection. */
      humanOverride: boolean;
    }
  | {
      kind: "needed";
      /** Every configured approver still needed (All), or the eligible pool (Any-of). */
      approvers: ApprovalReviewerRefDTO[];
      requireAll: boolean;
      /** All-of with configured humans: all of them have to approve. */
      allHumansRequired: boolean;
    };

/** One resolved set of recipients; slots with active requests are excluded. */
export interface ApprovalRecipientNeedsDTO {
  agent: boolean;
  humans: string[];
  /** Choose exactly one of these humans at random (Any-of) before display. */
  randomHumanFrom: string[];
}

export interface ApprovalRequestNeedsDTO {
  /** What Request all / Request outstanding approvals sends now. */
  outstanding: ApprovalRecipientNeedsDTO;
  /** What Request all sends when satisfied requirements are explicitly included. */
  repeat: ApprovalRecipientNeedsDTO;
  /** Agent configured with no active agent request. */
  agentRequestable: boolean;
  /** The agent's counted decision is approved. */
  agentApproved: boolean;
}

/** The request policy the server enforces (g5.governance.approval.request-pr). */
export interface ApprovalRequestPolicyDTO {
  prRequired: boolean;
  onSourceCommitVectorChange: "IGNORE" | "MAKE_PENDING";
}

/**
 * Server-evaluated merge admission (g5.governance.approval.requirements):
 * all attached target requirements gate the merge, evaluated
 * against the current source cut. Every shipping surface reads this rather
 * than recomputing it.
 */
export interface WorkspaceMergeAdmissionDTO {
  admitted: boolean;
  reason: string | null;
  /** When the server evaluated this verdict (ISO 8601). */
  evaluatedAt?: string;
  requirements: {
    definitionId: string;
    definitionName: string;
    definitionVersion: number;
    state: "approved" | "rejected" | "pending";
  }[];
}

/** Request-complete history independent of active merge requirements. */
export interface WorkspaceOutgoingHistoryDTO {
  definitionId: string;
  definitionName: string;
  requests: ApprovalRequestDTO[];
  decisionLog: ApprovalDecisionLogEntryDTO[];
}

/** An active requirement of the viewed workspace (target side). */
export interface WorkspaceRequiredIntoDTO {
  definition: OrgApprovalDefinitionDTO;
  attachedBy: { userId: string; displayName: string };
  attachedAt: string;
}

/** One attach/retire event (bottom history). */
export interface WorkspaceAttachmentEventDTO {
  definitionId: string;
  definitionName: string;
  retired: boolean;
  actor: { userId: string; displayName: string };
  createdAt: string;
}

/** Combined GET /api/v1/workspaces/:workspaceId/approvals state. */
export interface WorkspaceApprovalsStateDTO {
  requiredInto: WorkspaceRequiredIntoDTO[];
  attachmentHistory: WorkspaceAttachmentEventDTO[];
  outgoing: {
    targetWorkspaceId: string;
    targetWorkspaceName: string;
    requirements: WorkspaceOutgoingRequirementDTO[];
    history?: WorkspaceOutgoingHistoryDTO[];
    /** Merge admission against the current source cut (all target requirements). */
    mergeAdmission?: WorkspaceMergeAdmissionDTO;
    requestPolicy?: ApprovalRequestPolicyDTO;
  } | null;
}

/** Compact actionable row returned by the organization Home projection. */
export type HomeAttentionKind = "approval" | "open_question";

/** Page order of GET /orgs/:orgId/home/attention (g5.ui.home-attention-order). */
export type HomeAttentionOrder = "newest" | "oldest";

export interface HomeAttentionItem {
  id: string;
  kind: HomeAttentionKind;
  workspaceId: string;
  workspaceName: string;
  /**
   * The owning workspace's owner, resolved against the org roster for the
   * same owner avatar the Workspaces tile shows; absent (or unresolvable)
   * falls back to the deterministic initial, as the Workspaces panel does.
   */
  ownerUserId?: string | null;
  sourceId: string;
  displayName?: string;
  openQuestionCount?: number;
  actionableAt: string;
}

/** One bounded cursor page from GET /orgs/:orgId/home/attention. */
export interface HomeAttentionPage {
  items: HomeAttentionItem[];
  nextCursor: string | null;
}

// Base re-verification (ontology_and_harness g5.reverification) ------------

export type ReverificationKind = "incremental" | "full";

/** A run's state, read from its fork, workspace and pull requests. */
export type ReverificationRunState = "in_progress" | "completed" | "abandoned";

/** One code repository's Base commit at a run's fork point. */
export interface ReverificationHead {
  repoId: string;
  rootPath: string;
  commit: string;
}

export interface ReverificationRun {
  id: string;
  kind: ReverificationKind;
  state: ReverificationRunState;
  workspaceId: string;
  /** Null until the fork has created the run's workspace. */
  workspaceName: string | null;
  /** Deleted, as completed runs' workspaces usually are: name only, no link. */
  workspaceDeleted: boolean;
  /** The workspace's current owner. */
  ownerUserId: string | null;
  startedBy: string;
  startedAt: string;
  completedAt: string | null;
  /** The findings label, e.g. "Base re-verification 2026-11-06 (incremental)". */
  label: string;
  /** F: the Base commits the run forked from, per code repository. */
  baseHeads: ReverificationHead[];
  /** S: the previous completed run's `baseHeads`; incremental runs only. */
  sinceHeads: ReverificationHead[] | null;
}

/** A pull request merged into Base since the last completed run. */
export interface ReverificationPullRequest {
  id: string;
  number: number;
  title: string;
  mergedAt: string;
  mergedBy: string | null;
}

/**
 * GET /orgs/:orgId/home/base-reverification (g5.reverification.status):
 * only `enabled: false` while the organization's verification switch is off.
 */
export type BaseReverificationStatus =
  | { enabled: false }
  | {
      enabled: true;
      baseWorkspaceId: string;
      baseWorkspaceName: string;
      state: "never" | "current" | "stale" | "in_progress";
      /** The caller may fork Base, so may start a run. */
      canStart: boolean;
      lastCompleted: ReverificationRun | null;
      inProgress: ReverificationRun | null;
      /** Exact count since the last completed run's fork; 0 when never. */
      pullRequestCount: number;
      /** Newest first, capped at 200. */
      pullRequests: ReverificationPullRequest[];
      /** Net lines outside `.g5/`; null when never re-verified. */
      lines: { added: number; removed: number } | null;
      /** Some repository could not be diffed. */
      linesUnavailable: boolean;
      /** Newest first, capped at 50. */
      runs: ReverificationRun[];
      /** Every run on record, of which `runs` lists the newest. */
      runCount: number;
    };

/** Body for POST /api/v1/orgs/:orgId/approval-definitions (create v1). */
export interface CreateOrgApprovalDefinitionInput {
  requireAllReviewers?: boolean;
  humanApprovalsOverrideAgent?: boolean;
  allowSelfApproval?: boolean;
  name: string;
  description?: string;
  humanUserIds: string[];
  agentType?: "simple";
  agentPrompt?: string;
  /** Omitted fields receive the server's Claude Code / Opus 5.5 / high defaults. */
  agentHarness?: PickerHarness;
  agentModel?: string;
  agentEffort?: string;
  /**
   * Phase binding (g5.workbench.gates). Omitted resolves server-side to
   * `["ie"]`; the editor always sends it explicitly (empty = no recommendation).
   */
  phases?: WorkspacePhase[];
}

/**
 * Body for POST /api/v1/orgs/:orgId/approval-definitions/:definitionId/versions
 * (publish the next append-only version).
 */
export interface PublishOrgApprovalDefinitionVersionInput extends CreateOrgApprovalDefinitionInput {
  invalidatesPrevious: boolean;
}

/**
 * A tenant organization's platform facts from GET /api/v1/platform/orgs
 * (g5.organization.platform-operator.tenant-boundary).
 */
export interface PlatformOrgDTO {
  id: string;
  name: string;
  slug: string;
  createdAt: string;
  /** Active members. */
  memberCount: number;
}

/** Body for POST /api/v1/platform/orgs; the API derives an omitted slug. */
export interface CreatePlatformOrgInput {
  name: string;
  slug?: string;
  adminEmail: string;
}

/** Response from POST /api/v1/platform/orgs (create an org for an initial Admin). */
export interface CreatePlatformOrgResponse {
  org: { id: string; name: string; slug: string; createdAt: string };
  admin: {
    userId: string;
    email: string;
    /** True when G5 created the account; its first verified sign-in links it. */
    accountCreated: boolean;
  };
}

/** Org-scoped API key record returned by admin endpoints */
export interface ApiKey {
  id: string;
  orgId: string;
  userId: string;
  name: string;
  keyPrefix: string;
  scopes: string[];
  createdAt: string;
  expiresAt: string | null;
  lastUsedAt: string | null;
  revokedAt: string | null;
}

/** Request payload for creating an org API key */
export interface CreateApiKeyInput {
  name: string;
  scopes: string[];
  expiresInSecs?: number;
}

/** API key creation response including the one-time plaintext key */
export interface CreateApiKeyResult {
  apiKey: ApiKey;
  key: string;
}

/** Worker registration token scope */

// ---------------------------------------------------------------------------
// Worker Hosts (ORG-scoped cloud VM hosts)
//
// DISTINCT from the per-session `workers` concept above. A worker host is a
// bare cloud VM (one row per host) that boots workspace VMs; the control
// plane (g5-api) owns provisioning, status, and ownership. Mirrors the Rust
// DTOs in `g5-common/src/types/worker_host.rs` (camelCase wire form, snake_case
// status values). Optional fields are ABSENT (not null) when the backend has
// no value yet — hence `?:` rather than `| null`.
// ---------------------------------------------------------------------------

/** Lifecycle state of a worker host (snake_case wire values). */
export type WorkerHostStatus =
  | "provisioning"
  | "ready"
  | "unreachable"
  | "draining"
  | "terminated";

/** Worker-host runtime profile allowlist (mirrors backend migration 113). */
export type WorkerProfile = "fat-default" | "slim";

/**
 * One worker host record. Mirrors `WorkerHost` in g5-common column-for-column.
 * `cacheState`/`attestationState` are documented-but-untyped JSON blobs in V1.
 */
export interface WorkerHost {
  hostId: string;
  orgId: string;
  instanceType: string;
  region: string;
  kind?: "gce" | "self_hosted" | "macos_dev";
  slotsTotal?: number;
  slotSizeReported?: boolean;
  slotCpus?: number;
  slotMemMib?: number;
  scratchDiskMib?: number;
  runtimeIdentity?: Record<string, unknown>;
  capabilities?: string[];
  /** Derived from active workspace_vms, never a host-reported counter. */
  occupancy?: number;
  heartbeatState?: "never" | "live" | "stale";
  heartbeatAgeSecs?: number;
  /**
   * Whether the host's published runtime (module catalog, dispatcher contract)
   * matches the control plane's. Only `current` hosts receive new workspace
   * VMs; `unknown` means the host has not published one yet
   * (g5.runtime.placement.runtime-identity).
   */
  runtimeState?: "current" | "stale" | "unknown";
  status: WorkerHostStatus;
  createdAt: string;
  /** Who requested the host; absent for hosts created before it was recorded. */
  createdByUserId?: string;
  createdByDisplayName?: string;
  createdByEmail?: string;
  lastHeartbeat?: string;
  credentialExpiresAt?: string;
  credentialGeneration?: number;
  drainRequestedAt?: string;
  drainReason?: string;
  drainId?: string;
  lastError?: Record<string, unknown>;
  vpcId?: string;
  cloudInstanceId?: string;
  kernelVersion?: string;
  activeMicrovmCount?: number;
  cacheDiskUsedBytes?: number;
  cacheDiskTotalBytes?: number;
  cacheState?: unknown;
  attestationState?: unknown;
  /**
   * Classified cloud bring-up failure code. Present only when bring-up failed
   * while the row is still `provisioning`; mirrors `worker_hosts.provision_error_code`
   * (g5-api migration 102) and `WorkerHost.provision_error_code` in g5-common.
   * The UI's STATUS_DISPLAY map renders a precise badge (e.g. "No capacity"
   * for `zone_capacity_exhausted`) instead of leaving the row stuck on
   * "Provisioning". Codeset extends in lockstep with backend.
   */
  provisionErrorCode?: WorkerHostProvisionErrorCode;
  /**
   * Zone-stockout retry attempts consumed (`worker_hosts.provision_retry_count`,
   * migration 228). Always present (0 when never retried).
   */
  provisionRetryCount?: number;
  /**
   * Next scheduled stockout retry (`worker_hosts.provision_next_retry_at`,
   * migration 228). Present only while a retry is scheduled — the UI shows
   * "No capacity — retrying (attempt N)" instead of the terminal badge.
   */
  provisionNextRetryAt?: string;
  /**
   * Runtime/profile contract (PR1, ontology [D42]). Mirrors the backend
   * `WorkerHost` DTO; PR1 wires the wire contract only (no UI consumes these
   * yet — PR2 owns the UI). On the wire `workerProfile`/`runtimeLabels` are
   * always present and the rest are absent when unset; all are declared
   * optional here so existing consumers/fixtures that predate PR1 still
   * typecheck (PR2 may tighten when it builds the UI that reads them).
   */
  baseImageRef?: string;
  baseImageDigest?: string;
  /** Immutable provision-time source. Absent only on truthful legacy rows. */
  baseImageSourceKind?: "track" | "pin";
  baseImageSourceValue?: string;
  runtimeSchema?: string;
  runtimeLabels?: Record<string, unknown>;
  runtimeLabelsRefreshedAt?: string;
  workerProfile?: WorkerProfile;
  /**
   * The operator's explicit provision-time concurrency pin
   * (`worker_hosts.workspace_vm_limit`, migration 146). Absent ⇔ NULL ⇔ the host
   * tracks the current default budget, which the client derives from the
   * instance type (`defaultWorkspaceVmLimit(vcpusOf(instanceType))`) — the
   * server never serializes a resolved default (ontology [D55]).
   */
  workspaceVmLimit?: number;
}

/**
 * Known classified bring-up failure codes (mirror backend CHECK list,
 * migrations 102/228). `never_registered` is the stale-host detector's Pass 1
 * verdict (#782) and is rendered on terminated rows too; `zone_capacity_exhausted`
 * additionally renders a "retrying" variant while a [D66] retry is scheduled.
 */
export type WorkerHostProvisionErrorCode =
  | "zone_capacity_exhausted"
  | "abandoned_bringup"
  | "never_registered"
  | "base_image_below_glibc_floor";

/** Request body for POST /api/v1/orgs/:orgId/worker-hosts. */
export interface ProvisionWorkerHostInput {
  /** Cloud region; required. Must be one of WORKER_HOST_REGIONS. */
  region: string;
  /** GCE machine type; optional, defaults server-side to c3-standard-4. */
  instanceType?: string;
  /**
   * Optional digest-pinned base image (`<registry>/<image>@sha256:<64hex>`).
   * Mutually exclusive with `imageTrack`; tag-only/non-digest refs are
   * rejected 422.
   */
  baseImageRef?: string;
  /** Promoted image track, mutually exclusive with `baseImageRef`. */
  imageTrack?: string;
  /** Optional worker profile; omitted ⇒ server default `fat-default`. */
  workerProfile?: WorkerProfile;
  /**
   * Optional host VM budget — how many workspace VM limit (microVMs) the host runs,
   * from which the host self-sizes per-VM CPU/RAM (worker_hosts ontology [D53]).
   * Omitted ⇒ server defaults to the instance type's vCPU count; a value outside
   * `1..=vCPUs(instanceType)` is rejected `422`.
   */
  workspaceVmLimit?: number;
  /**
   * Optional disk size in GiB (the wire name is historical). The server
   * records at least H + V × slots; larger overrides are allowed, smaller
   * or unsupported sizes are rejected. Retries retain that saved allocation.
   */
  bootDiskGb?: number;
}

/**
 * 201 response from POST /api/v1/orgs/:orgId/worker-hosts.
 *
 * NOTE: `hostJwt` is a host credential returned exactly once. The UI MUST NOT
 * surface or persist it — it is intentionally omitted from this type so no
 * client code can read it off the result.
 */
export interface ProvisionWorkerHostResult {
  hostId: string;
  /** Saved allocation; optional only for older API responses. */
  diskAllocation?: {
    hostDiskGib: number;
    vmDiskMib: number;
    slots: number;
    diskGib: number;
  };
  status: WorkerHostStatus;
  region: string;
  instanceType: string;
  createdAt: string;
  /**
   * Runtime/profile contract echoed on the 201 (ontology [D42]). Mirrors the
   * Rust `ProvisionWorkerHostResponse`; PR1 wire contract only (no UI consumes
   * these yet). On the wire `workerProfile`/`runtimeLabels` are always present
   * and the rest are absent when unset; all are declared optional here so the
   * existing `useProvisionWorkerHost` reconstruction (which strips fields it
   * does not surface) still typechecks. PR2 may tighten when it consumes them.
   */
  baseImageRef?: string;
  baseImageDigest?: string;
  baseImageSourceKind: "track" | "pin";
  baseImageSourceValue: string;
  runtimeSchema?: string;
  runtimeLabels?: Record<string, unknown>;
  runtimeLabelsRefreshedAt?: string;
  workerProfile?: WorkerProfile;
}

/**
 * Boot-disk sizing coefficients the server publishes with the host list
 * (ontology `g5.runtime.placement.host-disk`).
 *
 * The Add worker host dialog computes its displayed default from THESE values
 * rather than from its own copy of the rule, so the size it shows cannot drift
 * from the size the server applies. The server resolves the organization's
 * allowances; the client has no policy constants to keep in sync.
 */
export interface WorkerHostBootDiskSizing {
  /** Fixed allowance independent of the VM budget. */
  baseGb: number;
  /** Provisioning allowance for one VM slot. */
  perSlotGb: number;
  /** Lower bound applied to the derived size. */
  floorGb: number;
  /** Smallest size an operator may submit. */
  minGb: number;
  /** Largest size an operator may submit, and the cap on the derived size. */
  maxGb: number;
}

/** 200 response from GET /api/v1/orgs/:orgId/worker-hosts. */
export interface ListWorkerHostsResponse {
  hosts: WorkerHost[];
  total: number;
  /**
   * Absent only against an API older than the per-slot disk sizing change; the
   * dialog then shows no derived default instead of inventing numbers.
   */
  bootDiskSizing?: WorkerHostBootDiskSizing;
  /**
   * The calling org's own default base image (digest-pinned), used to prefill
   * the Add Worker Host dialog. Absent when the org has no default. Never a
   * cross-org image list (worker_hosts ontology D50/D51).
   */
  orgDefaultBaseImageRef?: string;
  orgDefaultBaseImageTrack?: string;
  promotedImageTracks: WorkerImageTrack[];
}

/** Current non-stopped VM assignments on the org's Workers page. */
export interface WorkerHostWorkspace {
  workspaceId: string;
  workspaceName: string;
  hostId: string | null;
  /** Persisted VM status; queued intents on stopped VMs are excluded. */
  status: Exclude<WorkspaceVmStatus, "queued_for_capacity">;
  /** When the current VM became ready; null before Ready or without a VM. */
  startedAt: string | null;
  /** Latest recorded activity on this VM run. */
  lastActivityAt: string | null;
  ownerUserId: string | null;
  ownerDisplayName: string | null;
  ownerEmail: string | null;
}

/**
 * GET /api/v1/orgs/:orgId/compute/running — one non-stopped workspace VM the
 * caller may read (workbench Phase 6 Home compute tile). Same inventory as
 * `WorkerHostWorkspace`, without the owner e-mail and with the placed host's
 * slot size (`null` before placement).
 */
export interface AgentReviewInventoryLink {
  reviewId: string;
  sourceWorkspaceId: string;
  sourceWorkspaceName: string;
}

export interface RunningComputeDTO {
  agentReview?: AgentReviewInventoryLink | null;
  workspaceId: string;
  workspaceName: string;
  status: Exclude<WorkspaceVmStatus, "queued_for_capacity" | "stopped">;
  startedAt: string | null;
  lastActivityAt: string | null;
  ownerUserId: string | null;
  ownerDisplayName: string | null;
  hostId: string | null;
  slotCpus: number | null;
  slotMemMib: number | null;
  instanceType: string | null;
}

export interface WorkerImageTrack {
  name: string;
  imageRef: string;
  digest: string;
}

export interface UpdateWorkerImagePolicyInput {
  imageTrack?: string;
  baseImageRef?: string;
}

export interface WorkerImagePolicyResult {
  sourceKind: "track" | "pin";
  sourceValue: string;
  resolvedImageRef: string;
  resolvedDigest: string;
}

/** Regions a worker host may be provisioned in (matches the backend allowlist). */
export const WORKER_HOST_REGIONS = [
  "us-central1",
  "us-east1",
  "us-east4",
  "us-west1",
  "us-west2",
  "europe-west1",
  "europe-west4",
  "asia-east1",
  "asia-northeast1",
] as const;

/**
 * GCE machine types selectable for a worker host.
 *
 * MUST stay in lockstep with `g5-api-routes/src/routes/worker_hosts.rs`'s
 * `WORKER_HOST_INSTANCE_TYPE_ALLOWLIST` — if the backend accepts a SKU we
 * don't list here, the operator can't pick it from the UI; if we list a SKU
 * the backend doesn't accept, POST `/worker-hosts` 422s.
 *
 * Three families (per worker_hosts ontology [D19]/[D41], rev-7 §5.2): N2 (Intel
 * Cascade/Ice Lake — historical default), C2 (Intel Cascade Lake —
 * compute-optimized; higher per-core clock), and C3 (Intel Emerald Rapids —
 * latest-gen compute-optimized; added 2026-06-05 once GCP raised our project
 * quota, chat intent C1). C2 caps at `standard-30` (not `-32`) because that's
 * the real SKU ceiling for the family; C3 picks `-{4,8,22,44}` because its
 * standard SKUs skip `-16`/`-30` entirely (real GCE SKUs are 4/8/22/44/88/176;
 * 88+176 are off-list to keep the cost-vector ceiling near the C2 max).
 *
 * N2D (AMD EPYC) is intentionally excluded — GCE nested virtualization is
 * unavailable on AMD-based VMs, so N2D cannot run the nested KVM workload.
 *
 * The dialog offers ONLY the C3 family (a curated subset — see worker_hosts
 * ontology [D45]). N2/C2 hit region capacity/quota limits (the us-central1
 * C2_CPUS per-project-region cap of 14 repeatedly failed provisioning with
 * QUOTA_EXCEEDED); C3 is the standardized worker family across all
 * environments.
 *
 * This list is a strict SUBSET of the backend `WORKER_HOST_INSTANCE_TYPE_ALLOWLIST`
 * in `generation/g5-api-routes/src/routes/worker_hosts.rs` (which still accepts N2/C2/C3
 * for API/scripted callers). The surviving invariant: every value here MUST be in
 * the backend allowlist, else the provision call 422s. C3 ⊂ allowlist, so this
 * holds. Do NOT add a UI value that is not allowlisted.
 */
export const WORKER_HOST_INSTANCE_TYPES = [
  // C3 (Intel Emerald Rapids) — latest-gen compute-optimized; nested-virt
  // capable. SKUs skip -16/-30; the four tiers match the vCPU sizes the other
  // families cover. 88+176 are deliberately omitted (cost-vector ceiling).
  // Only C3 is surfaced in the UI ([D45]); the backend allowlist keeps N2/C2/C3.
  "c3-standard-4",
  "c3-standard-8",
  "c3-standard-22",
  "c3-standard-44",
] as const;

/**
 * Default GCE machine type — used for BOTH the "Add worker" dialog's initial
 * selection AND the API/DB omit-default (a provision request that omits
 * `instanceType`). Mirrors the backend `DEFAULT_WORKER_HOST_INSTANCE_TYPE`
 * (g5-common) and the `worker_hosts.instance_type` column DEFAULT (migration
 * 128). C3 is the standardized worker family ([D45]).
 */
export const DEFAULT_WORKER_HOST_INSTANCE_TYPE = "c3-standard-4";

/**
 * Worker profiles selectable for a worker host (PR3). Mirrors the backend
 * `worker_profile` CHECK allowlist exactly: `fat-default` (the full image) and
 * `slim`. A value the UI offers but the API rejects surfaces as a `422` from the
 * provision call; keep in lockstep with `g5-common`'s `worker_host.rs`.
 */
export const WORKER_PROFILES = ["fat-default", "slim"] as const;
/** Worker profile applied when a provision request omits workerProfile. Mirrors backend CHECK. */
export const DEFAULT_WORKER_PROFILE: WorkerProfile = "fat-default";

// ---------------------------------------------------------------------------
// 4. Workspace Types
// ---------------------------------------------------------------------------

/**
 * Workbench phase a workspace is in (workbench g5.workbench.phases): Business
 * (`be`), System (`se`) or Integration (`ie`) Engineering. Explicit,
 * owner-moved state (g5.workbench.phases.state); a fork copies its source's.
 */
export type WorkspacePhase = "be" | "se" | "ie";

// ---------------------------------------------------------------------------
// Workbench configuration (workbench g5.workbench.actions, g5.workbench.persona)
// — `GET /api/v1/workspaces/:id/workbench`. Mirrors
// `backend/g5-api-foundation/src/contracts/workbench.rs`.
// ---------------------------------------------------------------------------

/**
 * What a ribbon action does: `chat` starts a templated conversation under a
 * persona, `open` opens an existing workspace surface, `menu` groups actions
 * behind one dropdown button (`items`).
 */
export type WorkbenchActionKind = "chat" | "open" | "menu";

/** Existing surface an `open` action reaches through its owning handler. */
export type WorkbenchActionTarget =
  | "approvals"
  | "ship"
  | "merge"
  /** The Rebase Workspace dialog (ui g5.workbench.ship-actions). */
  | "rebase"
  | "preview"
  | "new-doc"
  | "upload"
  | "meeting"
  | "view"
  /** The Create Explainer modal (guided journey g5.workbench.journey.steps). */
  | "explainer"
  /** Seeds the Review Workspace prompt into the composer (same block). */
  | "review"
  /**
   * Seeds the PR re-verification prompt under its cost notice (ui
   * g5.workbench.review-prompts).
   */
  | "verification"
  /**
   * The Build mock dialog (g5.workbench.mock): this workspace's mocks under
   * Project Docs `mocks/`, newest first, and the seeded `generate-mock` prompt.
   */
  | "mock";

/** Context a `chat` action serializes into its message text. */
export type WorkbenchContextKind = "current_doc" | "phase" | "none";

export interface WorkbenchAction {
  id: string;
  label: string;
  kind: WorkbenchActionKind;
  /** Tinted with the phase hue; the group's main move. */
  primary?: boolean;
  /**
   * Reachable by id (a dialog or journey step starts it) but never drawn in
   * the ribbon (g5.workbench.meeting).
   */
  hidden?: boolean;
  /** Persona a `chat` action starts under (defaults to the ribbon phase). */
  persona?: WorkspacePhase;
  /** Template prompt of a `chat` action (a complete request as message text). */
  prompt?: string;
  context?: WorkbenchContextKind;
  /** Surface an `open` action reaches. */
  target?: WorkbenchActionTarget;
  /** Phase an approvals surface opens on. */
  phase?: WorkspacePhase;
  /** Center tab a `view` target opens. */
  tab?: string;
  /** Dropdown entries of a `menu` action (never nested further). */
  items?: WorkbenchAction[];
}

export interface WorkbenchActionGroup {
  name: string;
  actions: WorkbenchAction[];
}

/** Ribbon content per phase: labelled groups of actions. */
export type WorkbenchActions = Record<WorkspacePhase, WorkbenchActionGroup[]>;

// ---------------------------------------------------------------------------
// Guided journey marks (workbench g5.workbench.journey.marks) —
// `GET /api/v1/workspaces/:id/journey`,
// `PUT /api/v1/workspaces/:id/journey/steps/:stepId`. Mirrors
// `backend/g5-api-foundation/src/contracts/workbench.rs`.
// ---------------------------------------------------------------------------

/** The latest mark of one step: the owner's durable "Mark done" / "Not done". */
export interface JourneyMarkDTO {
  stepId: string;
  done: boolean;
  /** The owner skipped the step: not done, out of the count (g5.ui.guided-panel); absent on older servers. */
  skipped?: boolean;
  markedAt: string;
  actorUserId?: string | null;
}

export interface WorkspaceJourneyDTO {
  /** One entry per step that was ever marked (its latest event). */
  marks: JourneyMarkDTO[];
}

export interface WorkbenchPersona {
  source: "builtin" | "committed";
  text: string;
}

export type WorkbenchConfigStatus = "builtin" | "committed" | "invalid";

export interface WorkbenchConfigDTO {
  /** Committed docs head plus a hash of the built-ins; pinned by chat starts. */
  revision: string;
  actions: WorkbenchActions;
  personas: Record<WorkspacePhase, WorkbenchPersona>;
  config: { status: WorkbenchConfigStatus; error?: string };
}

/** Context a templated chat start serializes into its first message. */
export interface WorkbenchStartContext {
  kind: WorkbenchContextKind;
  docPath?: string;
}

/**
 * A workspace's immutable registry kind: the organization's Base, an ordinary
 * working workspace (fork, review copy), or a member's personal workspace
 * (personal_workspace g5.personal.identity). Never inferred from a name.
 */
export type WorkspaceKind = "base" | "working" | "personal";

/**
 * A member's personal workspace as `GET /orgs/:orgId/me` (ensure, owner) and
 * `GET /orgs/:orgId/users/:handle/workspace` (owner or administrators) report
 * it (personal_workspace g5.personal.routes).
 */
export interface PersonalWorkspaceDTO {
  workspaceId: string;
  orgId: string;
  userId: string;
  /** Always `users/<handle>` (g5.personal.names). */
  name: string;
  handle: string;
}

/** The Start a Start-and-send recorded (g5.personal.start-and-send). */
export interface StartComputeOutcome {
  intentId: string;
  /** Lifecycle status, e.g. `starting`, `queued_for_capacity`, `running`. */
  status: string;
}

/** Workspace list item DTO from GET /api/v1/orgs/:orgId/workspaces */
export interface WorkspaceSummary {
  agentReview?: AgentReviewInventoryLink | null;
  id: string;
  name: string;
  description: string | null;
  isBase: boolean;
  /**
   * Registry kind (personal_workspace g5.personal.identity); absent on older
   * servers. Read it through `workspaceKind`, which derives it from `isBase`.
   */
  kind?: WorkspaceKind;
  /**
   * True when the workspace is read-only (`mutation_policy = 'read_only'`).
   * Optional for backward/tolerant decoding (the API schema defaults it to
   * `false`); treat a missing value as read-write.
   */
  readOnly?: boolean;
  /**
   * True when the workspace is a review workspace (a reviewer-owned copy
   * created by Start review). Optional like `readOnly`; missing means no.
   */
  isReview?: boolean;
  /** Current exclusive workspace owner; creator is historical provenance only. */
  ownerUserId?: string | null;
  createdBy: string | null;
  clonedFromId: string | null;
  branches: BranchSummary[];
  tasksCompleted: number;
  totalTasks: number;
  runningTime: string | null; // human-readable duration
  cost: number; // total cost in USD
  concurrentTasks: number;
  lastUpdated: string; // ISO 8601
  createdAt: string; // ISO 8601
  /**
   * Active primary external issue (association only). Present on workspace
   * list responses; absent from single-workspace and fork responses.
   */
  primaryIssue?: WorkspacePrimaryIssueDTO | null;
  /** Workbench phase (g5.workbench.phases.state); absent on older servers. */
  phase?: WorkspacePhase;
}

/** A workspace's primary external issue as the workspace list reports it. */
export interface WorkspacePrimaryIssueDTO {
  /** Absent on older servers, which serve GitHub issues only. */
  provider?: IssueProvider;
  linkId: string;
  displayKey: string;
  canonicalUrl: string;
  locator: ExternalIssueLocatorDTO;
}

/** Aggregated open-question counts for one workspace. */
export interface WorkspaceOpenQuestionMetrics {
  workspaceId: string;
  /** Embedded open fences plus open or proposed question-set entries, each
   * set entry counted once (`g5.documents.question.metrics`). */
  documentOpenQuestions: number;
  /** Equals `documentOpenQuestions`; Project Docs are the only source. */
  openQuestionsTotal: number;
  projectDocSources: ProjectDocOpenQuestionSource[];
  /** The question-set family; absent on older servers. */
  questionSetSources?: QuestionSetOpenSource[];
  /** A scanned set did not parse: the counts are a lower bound. */
  questionSetsIncomplete?: boolean;
}

export interface QuestionSetOpenSource {
  path: string;
  setId: string;
  title: string;
  openQuestionCount: number;
  openQuestions: QuestionSetOpenQuestion[];
  /** Stable doc ids of the referencing Project Docs. */
  referencedBy: string[];
  invalid: boolean;
}

export interface QuestionSetOpenQuestion {
  questionId: string;
  displayNumber: number;
  title: string;
  severity: UncertaintySeverity | null;
  status: "open" | "proposed";
}

export interface ProjectDocOpenQuestionSource {
  docId: string;
  displayName: string;
  openQuestionCount: number;
  firstOpenQuestionBlockId: string | null;
  openQuestions: ProjectDocOpenQuestion[];
}

export interface ProjectDocOpenQuestion {
  blockId: string;
  title: string;
  severity: UncertaintySeverity | null;
}

/** Workspace detail for single-workspace view */
export interface WorkspaceDetail {
  id: string;
  orgId: string;
  name: string;
  isBase: boolean;
  /**
   * Registry kind (personal_workspace g5.personal.identity); absent on older
   * servers. Read it through `workspaceKind`, which derives it from `isBase`.
   */
  kind?: WorkspaceKind;
  /**
   * True when the workspace is read-only (`mutation_policy = 'read_only'`).
   * Optional for tolerant decoding (API schema defaults it to `false`).
   */
  readOnly?: boolean;
  /**
   * Transient transition marker: `"freezing"` while a read-write → read-only
   * drain is in progress, otherwise absent. Distinct from `readOnly` (a freezing
   * workspace is still `readOnly:false`). Drives the "Freezing…" indicator, the
   * escape-hatch "Make editable" control, and detail polling
   * (@ref software_architecture/g5-api/routes/workspaces#D30).
   */
  mutationPolicyTransition?: string;
  description: string | null;
  clonedFromId: string | null;
  docsRepoId: string | null;
  createdBy: string | null; // user ID, not UserRef
  /** Current exclusive workspace owner; creator is historical provenance only. */
  ownerUserId?: string | null;
  createdAt: string;
  updatedAt: string;
  /** Per-workspace outbound push mode: 'manual' | 'auto' (default). */
  pushMode: PushMode;
  /**
   * Workbench phase (g5.workbench.phases.state). Present on GET detail and
   * phase-update responses; absent on older servers and other mutations.
   */
  phase?: WorkspacePhase;
  branches: BranchSummary[];
  /** Dedicated workspace docs folder convention, always "docs/<id>" (computed server-side). */
  docsFolderPath: string;
  /**
   * Names the workspace held before and still reserves, newest first
   * (g5.workspace.former-names). Absent when it was never renamed.
   */
  formerNames?: string[];
}

export type WorkspaceVmStatus =
  | "stopped"
  | "starting"
  | "queued_for_capacity"
  | "running"
  | "stopping"
  | "failed";

export interface WorkspaceVmView {
  orgId: string;
  workspaceId: string;
  ownerUserId: string;
  status: WorkspaceVmStatus;
  hostId: string | null;
  vmId: string | null;
  epoch: number;
  imageRef: string | null;
  moduleCatalogVersion: string | null;
  startIntent: Record<string, unknown> | null;
  startDeadlineAt: string | null;
  startedAt: string | null;
  lastActivityAt: string | null;
  stopReason: string | null;
  stopOperationId: string | null;
  stopDeadlineAt: string | null;
  pendingStopReason: string | null;
  pendingStopDeadlineAt: string | null;
  nudgedAt: string | null;
  lastFlushId: string | null;
  unflushedLossAt: string | null;
  lastError: Record<string, unknown> | null;
}

export type WorkspaceResumeOutcome = {
  outcome: "attached" | "starting" | "queued_for_capacity";
  vm: WorkspaceVmView;
};

export interface WorkspaceStopOutcome {
  operationId: string;
  status: string;
  flushId: string | null;
  unflushedLoss: boolean;
}

// ---------------------------------------------------------------------------
// Review workspaces (`/api/v1/workspaces/:id/review`)
// ---------------------------------------------------------------------------

/**
 * A reviewer's review workspace for one source workspace: the reviewer-owned
 * durable fork that tracks the source workspace branch as its base. Derived
 * server-side from the completed review fork; nothing is stored beyond the
 * fork itself.
 */
export interface WorkspaceReview {
  forkId: string;
  orgId: string;
  sourceWorkspaceId: string;
  sourceWorkspaceName: string;
  reviewWorkspaceId: string;
  reviewWorkspaceName: string;
  reviewerUserId: string;
  reviewerDisplayName: string;
  /** `workspace_vms.status` of the review workspace; `stopped` when it has no incarnation. */
  vmStatus: WorkspaceVmStatus | string;
  vmStartedAt: string | null;
  vmLastActivityAt: string | null;
  /** When the review workspace's state was last flushed durably (what Reopen restores). */
  lastFlushedAt: string | null;
  /** When the review workspace was forked. */
  createdAt: string;
}

/** GET /api/v1/workspaces/:id/review */
export interface WorkspaceReviewContext {
  /** The caller's review of this workspace, when they started one. */
  asSource: WorkspaceReview | null;
  /** This workspace's review origin, when it is a review workspace. */
  asReview: WorkspaceReview | null;
  /** Open suggestions waiting on this workspace (as a source). */
  openSuggestions: number;
}

/** POST /api/v1/workspaces/:id/review/start */
export interface StartWorkspaceReviewResponse {
  review: WorkspaceReview;
  /** `true` when this call forked the review workspace, `false` when it was reopened. */
  created: boolean;
  resume: WorkspaceResumeOutcome | null;
  /** Why the VM was not resumed; the review workspace exists regardless. */
  resumeError: string | null;
}

// ---------------------------------------------------------------------------
// Review publications, findings and suggestions
// ---------------------------------------------------------------------------

export type ReviewVerdict = "approve" | "request_changes" | "comment_only";
export type ReviewAuthorKind = "human" | "platform";
export type ReviewSuggestionKind = "hunk" | "new_file" | "comment" | "artifact";
export type ReviewSuggestionStatus =
  | "open"
  | "partial"
  | "accepted"
  | "rejected"
  | "withdrawn"
  | "superseded"
  | "obsolete"
  | "resolved";
export type ReviewFindingStatus =
  | "open"
  | "accepted"
  | "rejected"
  | "superseded";
export type ReviewSeverity = "P0" | "P1" | "P2" | "P3";

/**
 * One repository of the source commit a publication is about
 * (`baseCommitVector`): a round records `sourceHead` at push time, an
 * annotation the head it was made against.
 */
export interface ReviewBaseCommitEntry {
  repoId: string;
  sourceHead?: string;
  mergeBase?: string;
  reviewHead?: string;
  sourceBranch?: string;
  reviewBranch?: string;
}

/** One round of one reviewer's review of a source workspace. */
export interface ReviewPublication {
  id: string;
  orgId: string;
  sourceWorkspaceId: string;
  /** `null` for a comments-only publication (no review workspace). */
  reviewWorkspaceId: string | null;
  reviewerUserId: string;
  authorKind: ReviewAuthorKind | string;
  authorLabel: string;
  round: number;
  /** `ReviewBaseCommitEntry[]` when the server wrote it; parse defensively. */
  baseCommitVector: ReviewBaseCommitEntry[] | unknown;
  summary: string;
  verdict: ReviewVerdict | string | null;
  scores: Record<string, unknown> | null;
  supersededBy: string | null;
  createdAt: string;
  /** `round` (Push review) or `annotation` (comments, uploads). */
  kind?: "round" | "annotation" | string;
}

export interface ReviewPublicationSummary extends ReviewPublication {
  openCount: number;
  acceptedCount: number;
  rejectedCount: number;
  resolvedCount: number;
  supersededCount: number;
  artifactCount: number;
}

export interface ReviewFinding {
  id: string;
  publicationId: string;
  title: string;
  severity: ReviewSeverity | string;
  dimension: string;
  evidence: string;
  actions: unknown;
  status: ReviewFindingStatus | string;
  position: number;
  createdAt: string;
  /** Where it points: the repository (when named) and the paths it concerns. */
  repoId: string | null;
  paths: string[];
}

/** A durable source edit. Offsets are UTF-8 bytes within the parent's source. */
export interface ReviewSuggestionEdit {
  id: string;
  start: number;
  end: number;
  beforeText: string;
  afterText: string;
  acceptedAfterText: string | null;
  status: ReviewSuggestionStatus | string;
  statusBy: string | null;
  statusAt: string | null;
  commentId: string | null;
  checkpointPending: boolean;
  writtenHash: string | null;
  /** Derived by the review owner when applied source correspondence conflicts. */
  sourceConflict?: boolean;
  /** UTF-8 offsets within expectedBeforeLines, after accepted siblings. */
  currentStart: number;
  currentEnd: number;
  expectedBeforeLines?: string;
  /** Only this edit applied to expectedBeforeLines; open siblings stay out. */
  expectedAfterLines?: string;
}

export interface ReviewSuggestion {
  id: string;
  publicationId: string;
  findingId: string | null;
  repoId: string | null;
  path: string | null;
  kind: ReviewSuggestionKind | string;
  beforeLines: string | null;
  afterLines: string | null;
  anchorStart: number | null;
  contentHash: string | null;
  anchorContext: string | null;
  authorKind: ReviewAuthorKind | string;
  authorLabel: string;
  note: string;
  commentId: string | null;
  status: ReviewSuggestionStatus | string;
  statusBy: string | null;
  statusAt: string | null;
  supersedes: string | null;
  adoptedFrom: string | null;
  createdAt: string;
  /** Backend-owned independent decisions within this immutable publication. */
  edits?: ReviewSuggestionEdit[];
  /** Current parent source with accepted siblings, shared by all child edits. */
  expectedBeforeLines?: string;
  /** Display adapter routing. The server's parent ID remains the URL target. */
  parentSuggestionId?: string;
  editId?: string;
  reviewEdit?: ReviewSuggestionEdit;
  parentBeforeLines?: string | null;
  parentAfterLines?: string | null;
  parentCommentId?: string | null;
  /** Remaining open children affected by the retained parent withdrawal. */
  parentOpenEditCount?: number;
  /** artifact: Git blob id in the repository's blob store. */
  blobOid?: string | null;
  blobSha256?: string | null;
  bytes?: number | null;
  contentType?: string | null;
  /** The reviewer who published it (on the source's suggestion list). */
  reviewerUserId?: string | null;
}

/** One change of a previewed round, as the reviewer's own review panel shows it before it is pushed. */
/**
 * The row of the reviewer's current round that already carries a previewed
 * change: their latest round, only while the source is still at the commit
 * it reviewed. The panel shows it as "In round N" and counts it as pushed.
 */
export interface ReviewPublishedIn {
  publicationId: string;
  round: number;
  suggestionId: string;
  /** The row's status: `open`, `accepted`, `rejected`, ... */
  status: string;
}

/**
 * What a push would carry for a previewed change whatever the source's
 * commit: the row it continues and the note it inherits when the push
 * names none. The note editor shows that note so it can be changed or
 * cleared; `publishedIn` says whether the change is already in the review.
 */
export interface ReviewCarried {
  suggestionId: string;
  round: number;
  note: string;
}

export interface ReviewPreviewChange {
  /** Names this change to `changeNotes` / `excludedChanges` of publish: a digest of path and text, stable across previews. */
  key: string;
  /** Authoritative discrete edit count; preview keys still identify Git hunks. */
  editCount?: number;
  /** Proven continuation units; a subset does not confer a whole-hunk marker. */
  carriedEditCount?: number;
  /** Proven pending continuation units, excluding settled decisions. */
  carriedOpenEditCount?: number;
  /** `hunk` or `new_file`. */
  kind: "hunk" | "new_file" | string;
  /** First line of the replaced block in the source file (hunks). */
  anchorStart: number | null;
  beforeLines: string | null;
  afterLines: string;
  /** Where this change already is, or null for a change no current round has. */
  publishedIn?: ReviewPublishedIn | null;
  /** What a push would carry for this change, or null. */
  carried?: ReviewCarried | null;
}

/** POST /api/v1/workspaces/:reviewWorkspaceId/review/preview */
export interface ReviewPreviewFile {
  repoId: string;
  path: string;
  /** `hunk` (changed file), `new_file` (a new text file the owner decides on) or `artifact` (a new binary or large file, opened or downloaded). */
  kind: "hunk" | "new_file" | "artifact" | string;
  hunks: number;
  bytes: number | null;
  /** Each hunk of a changed file, or the one new-file proposal; artifacts carry none. */
  changes: ReviewPreviewChange[];
  /** A new file's or an artifact's key for discard; null for a changed file (its hunks carry theirs). */
  key?: string | null;
  /** For a new file or an artifact: where it already is (a changed file answers per hunk). */
  publishedIn?: ReviewPublishedIn | null;
  /** For a new file or an artifact: what a push would carry for it. */
  carried?: ReviewCarried | null;
}

/**
 * Where a review copy stands against its source, per repository: the
 * source branch's tip in the mirror and where the copy branched from it. A
 * merge base behind the source head means the source moved on.
 */
export interface ReviewPreviewBaseEntry {
  repoId: string;
  sourceBranch?: string;
  sourceHead?: string;
  reviewBranch?: string;
  reviewHead?: string;
  mergeBase?: string;
}

export interface ReviewPreview {
  files: ReviewPreviewFile[];
  skipped: ReviewSkippedFile[];
  baseCommitVector?: ReviewPreviewBaseEntry[];
  /**
   * How the reviewer's current work was saved before this preview: in the
   * running review workspace (`live`), or not at all because it is stopped
   * (`stopped`: what it saved before stopping is what a push sends).
   */
  saved?: { workspace: "live" | "stopped" | string; committed: boolean };
}

/** A finding the reviewer declares when pushing a review. */
export interface NewReviewFinding {
  title: string;
  severity?: ReviewSeverity;
  dimension?: string;
  evidence?: string;
  actions?: unknown;
  /** Changed paths this finding explains; their hunks are grouped under it. */
  paths?: string[];
  /**
   * The repository those paths live in. Without it the finding claims the
   * path in every repository of the workspace.
   */
  repoId?: string;
}

/** POST /api/v1/workspaces/:reviewWorkspaceId/review/publish */
/**
 * POST /api/v1/workspaces/:reviewWorkspaceId/review/changes/discard — take
 * previewed changes out of the copy's working tree (a rejected proposal
 * would otherwise stay a difference against the source forever). Every
 * item is named by a preview key: a hunk's own, or the file-level key of a
 * new file or an artifact.
 */
export interface DiscardReviewChangesInput {
  /** Preview keys: a hunk's `changes[].key`, a new file's or artifact's file-level `key`. */
  keys: string[];
}

export interface DiscardReviewChangesResponse {
  discarded: { key: string; repoId: string; path: string; kind: string }[];
  skipped: { key: string; reason: string }[];
}

export interface PublishWorkspaceReviewInput {
  summary?: string;
  verdict?: ReviewVerdict;
  scores?: Record<string, unknown>;
  /** Per-path notes shown on the generated suggestions. */
  notes?: Record<string, string>;
  findings?: NewReviewFinding[];
  /** Changed paths to leave out of this round. */
  excludedPaths?: string[];
  /** Notes per change key (the preview's `changes[].key`); a change's own note outranks its path's. */
  changeNotes?: Record<string, string>;
  /** Change keys to leave out of this round. */
  excludedChanges?: string[];
  /** Files (one repository's path each) to leave out of this round. */
  excludedFiles?: ExcludedReviewFile[];
}

/** One repository's file, as Push review leaves it out. */
export interface ExcludedReviewFile {
  repoId: string;
  path: string;
}

export interface ReviewSkippedFile {
  repoId: string;
  path: string;
  reason: string;
}

export interface PublishWorkspaceReviewResponse {
  publication: ReviewPublication;
  findings: ReviewFinding[];
  suggestions: ReviewSuggestion[];
  skipped: ReviewSkippedFile[];
}

/**
 * POST /api/v1/workspaces/:id/review/file-suggestions: a reader's edits to
 * one text file, pushed to the owner as review suggestions with no review
 * workspace (`g5.governance.file-suggestions`).
 */
export interface PushFileSuggestionsInput {
  repoId: string;
  path: string;
  /** The text the reader started from, as read. */
  baseContent: string;
  /** The reader's edited text. */
  content: string;
  /** Opens each suggestion's thread; optional. */
  note?: string;
  /** Push against `baseContent` even though the file changed since. */
  allowStaleBase?: boolean;
  /**
   * The principal the edit was made under; it is never sent under another
   * principal's token (`g5.security.login.principal`). Not part of the body.
   */
  principal?: string | null;
}

export interface PushFileSuggestionsResponse {
  publication: ReviewPublication;
  suggestions: ReviewSuggestion[];
}

/** GET /api/v1/workspaces/:id/review/publications */
export interface ReviewPublicationsResponse {
  publications: ReviewPublicationSummary[];
  findings: ReviewFinding[];
  reviewerDisplayNames: Record<string, string>;
  /**
   * The source's current commit per repository. A publication whose vector
   * names another `sourceHead` is outdated: its rows leave the file pages
   * and stay on the Reviews page under their commit.
   */
  sourceHeads: Record<string, string>;
  /**
   * When each current source head was committed (RFC 3339), per repository:
   * a decision taken after it cannot be in that commit.
   */
  sourceHeadTimes?: Record<string, string>;
}

export interface ReviewSuggestionFilter {
  publicationId?: string;
  findingId?: string;
  repoId?: string;
  path?: string;
  openOnly?: boolean;
  /** Only rows about the source's current commit (what a file page shows). */
  currentOnly?: boolean;
  /** Only rows of this kind (`hunk`, `new_file`, `artifact`, `comment`). */
  kind?: string;
  /** Only rows in this status (`accepted` artifacts: the kept ones). */
  status?: string;
}

/** GET /api/v1/workspaces/:id/review/suggestions */
export interface ReviewSuggestionsResponse {
  suggestions: ReviewSuggestion[];
}

export interface ReviewSuggestionPathCount {
  repoId: string;
  path: string;
  openCount: number;
}

/** GET /api/v1/workspaces/:id/review/suggestions/counts */
export interface ReviewSuggestionCountsResponse {
  paths: ReviewSuggestionPathCount[];
}

/** Result of accept / reject / resolve / withdraw on one suggestion. */
export interface ReviewSuggestionResult {
  suggestion: ReviewSuggestion;
  /** Hash of the file after an accept wrote it; `null` otherwise. */
  writtenHash: string | null;
  /** The live file already contained the change; nothing was written. */
  alreadyApplied: boolean;
  /** The file was applied but its automatic commit/push needs recovery. */
  checkpointPending?: boolean;
}

export interface ReviewSuggestionReply {
  id: string;
  commentId: string;
  authorId: string;
  authorName: string;
  authorAvatar: string | null;
  content: string;
  createdAt: string;
}

export interface WorkspaceTransferOutcome {
  transferId: string;
  workspaceId: string;
  fromUserId: string;
  toUserId: string;
  requestedBy: string;
  status: string;
  completedAt: string | null;
}

/** Workspace stats from GET /api/v1/workspaces/:id/stats */
export interface WorkspaceStats {
  totalCostUsd: number;
  tasksCompleted: number;
  totalTasks: number;
  concurrentTasks: number;
  /** Tasks in the `cancelled` terminal state (e.g. a stopped chat). */
  tasksStopped: number;
  /**
   * Retained task costs, ordered by cost descending. `totalCostUsd` also
   * includes ordinary conversation costs without a task identity.
   */
  byTask: TaskCostBreakdown[];
  branches: Array<{
    workspaceId: string;
    repoId: string;
    repoName: string;
    branchName: string;
    /** `null` = unknown; the stats endpoint computes no divergence (D50). */
    aheadOfBase: number | null;
    /** `null` = unknown; the stats endpoint computes no divergence (D50). */
    behindBase: number | null;
  }>;
}

/** Target policy for a workspace read-only transition. */
export type WorkspaceMutationPolicy = "read_only" | "read_write";

/**
 * Request body for `POST /api/v1/workspaces/:id/mutation-policy`.
 * Mirrors the server `MutationPolicyTransitionRequestDTO`
 * (@ref software_architecture/g5-api/routes/workspaces#I5c).
 */
export interface SetWorkspaceMutationPolicyInput {
  policy: WorkspaceMutationPolicy;
  /** Concurrency mode, RW→RO only: 'drain' (default) or 'require_quiescent'. */
  mode?: "drain" | "require_quiescent";
  /** Drain timeout in seconds, RW→RO drain only (server default 300). */
  drainTimeoutSec?: number;
}

/** One outstanding blocker in a transition response. */
export interface TransitionBlocker {
  kind: string;
  id: string;
  detail: string;
}

/**
 * Response body for `POST /api/v1/workspaces/:id/mutation-policy`.
 * Mirrors the server `MutationPolicyTransitionResponseDTO`.
 */
export interface MutationPolicyResult {
  policy: WorkspaceMutationPolicy;
  /** Transient transition marker ('freezing') while a RW→RO drain is in flight. */
  transition?: string;
  /**
   * Human-readable one-line summary of the outcome (committed / draining /
   * refused-with-reason). Always present; shown to the operator verbatim.
   */
  message: string;
  /**
   * Outstanding blockers on a require_quiescent refusal, an up-front precondition
   * refusal (dirty drafts), or a drain-timeout revert. Each carries an
   * actionable `detail`.
   */
  blockers: TransitionBlocker[];
  /** Number of tasks that terminalized during the drain window. */
  drainedTasks?: number;
}

/** Branch summary for workspace list views */
export interface BranchSummary {
  workspaceId: string;
  repoId: string;
  repoName: string;
  branchName: string;
  trackingMode: string; // 'tracking' | 'pinned' | 'remote_tracking'
  headCommit: string; // SHA
  /**
   * Commits ahead of the merge-target-else-clone-parent workspace, or `null`
   * when unknown. Only the workspace-detail endpoint computes real values;
   * no surface fabricates a `0` (workspace_service ontology D50, issue #1995).
   */
  divergence: number | null;
  /**
   * Commits behind the comparison workspace, or `null` when unknown. Never
   * treat `null` as `0` — the Prepare-to-Ship badge must not render green
   * from an unknown value (prepare-to-ship-modal ontology D3).
   */
  behindBase: number | null;
  hasDraft: boolean; // true when outstanding draft edits exist
  dirtyFileCount: number; // files with uncommitted changes
  /**
   * Remote-side branch name (`remote_integrations.remote_branch`). Present
   * only for `trackingMode === "remote_tracking"` from the workspace-detail
   * endpoint. The ReposBar shows it in place of the G5 canonical local name.
   */
  remoteBranch?: string;
  /**
   * Provider repository full name (`provider_full_name`, e.g.
   * `octocat/hello-world`, or `owner/repo` parsed from the clone URL). Present
   * only for `trackingMode === "remote_tracking"`. The ReposBar shows it in
   * place of the G5 `org/repoName` identity.
   */
  remoteRepoName?: string;
  /**
   * Option 1 (G5 #1198): authoritative per-child passive auto-pull setting.
   * Present only for `trackingMode === "remote_tracking"` children. The client
   * treats this server value as authoritative after refresh so an optimistic
   * toggle never masquerades as confirmed state.
   */
  autoPullEnabled?: boolean;
  /**
   * Option 2 (G5 #1198): `true` when this local-only branch can be published to
   * a new remote ref — i.e. its fork parent had this repo remote-tracked.
   * Present only for provenance-eligible pinned branches; the chip offers
   * "Publish" only when this is `true`.
   */
  remoteEligible?: boolean;
}

/** Branch detail for workspace detail view */
export type BranchDetail = BranchSummary;

/** Canonical repository membership with current Git state for file readers. */
export interface WorkspaceBranchDetail {
  workspaceId: string;
  repoId: string;
  repoName: string;
  branchName: string;
  headCommit: string;
  trackingMode: string;
  materializationStatus: string;
  hasDraft: boolean;
  dirtyFileCount: number;
  dirtyFiles: string[];
}

/** Dependency edge between workspaces */
export interface DepEdge {
  from: string; // workspace ID
  to: string; // workspace ID
}

/** Request body for POST /api/v1/workspaces/:id/clone */
export interface CloneWorkspaceInput {
  name: string;
  /** Stable across retries of the same exact fork request. */
  idempotencyKey: string;
  /** Durable Git is the default; live capture is available only to the source owner. */
  source?: "durable" | "live";
  /** Permit interruption when an owner explicitly chooses live capture. */
  interrupt?: boolean;
  description?: string;
  /**
   * When `true`, create the forked workspace read-only (`mutation_policy =
   * 'read_only'`). Omitted / `false` forks read-write; a clone never inherits
   * the source workspace's read-only policy.
   */
  readOnly?: boolean;
  /**
   * Org workspace the fork intends to merge into. Omitted means "merge to the
   * fork parent" (`g5-api/services/clone_service` D32).
   */
  mergeTargetWorkspaceId?: string;
  /** Selected source sessions copied as part of the same durable fork. */
  chatSessionIds?: string[];
  /**
   * An existing external issue to link to the fork when it completes. `ref`
   * is a GitHub issue URL or `owner/name#number` in a configured issue
   * domain; it is resolved when the fork is admitted.
   */
  issue?: { ref: string; role: WorkspaceIssueLinkRole };
  /**
   * `name` is a suggestion: when it is taken, the fork takes the first free
   * `name-2`, `name-3`, …; the response names the workspace created.
   */
  uniquifyName?: boolean;
  /**
   * Start a chat in the new workspace with this first message. The launch
   * preferences are the confirmed choices a chat start sends; the fork
   * resolves them again for the new workspace.
   */
  startChat?: {
    content: string;
    launchPreferences?: AgentLaunchPreferencesV1;
  };
  /**
   * Start a Base re-verification run with this fork, admitted together
   * (g5.reverification.run); Base is the only source that accepts it.
   */
  reverification?: { kind: ReverificationKind };
}

/** The chat a fork started in the workspace it created. */
export interface ForkStartedChatDTO {
  /** Set when the first message was queued. */
  sessionId: string | null;
  /** Why the chat did not start; the workspace is ready either way. */
  error: string | null;
  /** Why the new workspace was not started; its message stays queued. */
  workspaceStartError: string | null;
}

/** The issue association a completed fork created. */
export interface ForkIssueLinkOutcomeDTO {
  linkId: string;
  displayKey: string;
  canonicalUrl: string;
  requestedRole: WorkspaceIssueLinkRole;
  /** `related` when another workspace claimed the issue as primary while the fork ran. */
  role: WorkspaceIssueLinkRole;
}

/** Result from POST /api/v1/workspaces/:id/sync */
export interface SyncWorkspaceResult {
  branchesUpdated: number;
  branchesSkipped: number;
  details: SyncBranchDetail[];
}

export interface SyncBranchDetail {
  workspaceId: string;
  repoId: string;
  repoName: string;
  branchName: string;
  status: string;
  oldCommit: string | null;
  newCommit: string | null;
  /**
   * The half-merged workspace PR holding this repository's auto-pull; an
   * explicit Pull imports it anyway, alone (g5.governance.remote.pull).
   */
  heldBy?: ImportHold;
}

/** A workspace PR merged on some repositories' providers but not all of them. */
export interface ImportHold {
  pullRequestId: string;
  title: string;
}

/**
 * Why a remote-tracked source repository's captured head may not be what a
 * fork's requester expects (g5.fork.source.advisory). Never blocks forking.
 */
export interface ForkSourceAdvisory {
  repoId: string;
  repoName: string;
  kind: "import_held" | "paused" | "behind";
  pullRequest?: ImportHold;
  pauseReason?: string;
  providerTip?: string;
}

/** Per-workspace coordinated git operation evidence. */
export interface CoordinatedOperationResult {
  operation: string;
  status: string;
  message?: string;
  repos: CoordinatedRepoEvidence[];
}

/** Per-repo evidence for a coordinated workspace operation. */
export interface CoordinatedRepoEvidence {
  workspaceId?: string;
  repoId: string;
  repoName: string;
  branchName: string;
  remoteBranch?: string;
  operation: string;
  status: string;
  oldCommit?: string | null;
  newCommit?: string | null;
  error?: string;
}

/**
 * Outcome discriminator for the promote / Make-Remote endpoint
 * (workspace_service D47):
 * - `"bound"` — ordinary equal/remote-ahead promotion ([D28] path);
 * - `"published"` — Make-Remote absent-ref create-and-bind;
 * - `"merge_required"` — Make-Remote divergent / local-ahead lineage routed
 *   into an operator-driven remote-tracking merge run.
 */
export type PromoteRemoteTrackingOutcome =
  | "bound"
  | "published"
  | "merge_required";

/** Fields shared by every promote / Make-Remote outcome. */
interface PromoteRemoteTrackingResponseBase {
  branch: BranchSummary;
  coordinated: CoordinatedOperationResult;
}

/**
 * Non-divergent promotion outcomes: ordinary [D28] bind (`"bound"`) and
 * Make-Remote absent-ref create-and-bind (`"published"`). Neither opens a merge
 * run, so this variant never carries a `mergeRunId`.
 */
export interface PromoteRemoteTrackingBoundResponse extends PromoteRemoteTrackingResponseBase {
  outcome: "bound" | "published";
}

/**
 * Divergent / local-ahead / unrelated-lineage outcome (D46 §3). The backend
 * synchronously opened a remote-tracking merge run, so `mergeRunId` is REQUIRED
 * on this variant — the UI routes it into the workspace-merge-workflow.
 */
export interface PromoteRemoteTrackingMergeRequiredResponse extends PromoteRemoteTrackingResponseBase {
  outcome: "merge_required";
  mergeRunId: string;
}

/**
 * Response shape of the existing-branch remote binding endpoint, modeled as a
 * discriminated union on `outcome` (D46): only the `"merge_required"` variant
 * carries `mergeRunId`, so the compiler forces callers to prove the outcome
 * before reading it. The wire payload is an untyped cast (no zod parse on this
 * path), so a runtime `"merge_required"` response can still arrive malformed
 * without a `mergeRunId`; consumers must guard for that at runtime.
 */
export type PromoteRemoteTrackingResponse =
  | PromoteRemoteTrackingBoundResponse
  | PromoteRemoteTrackingMergeRequiredResponse;

/** Result from POST /api/v1/workspaces/:id/merge */
export interface MergeWorkspaceResult {
  branchesMerged: number;
  branchesFailed: number;
  details: MergeBranchDetail[];
}

export interface MergeBranchDetail {
  workspaceId: string;
  repoId: string;
  repoName: string;
  branchName: string;
  status: string;
  mergeCommit: string | null;
  error: string | null;
}

/** Request body for POST /api/v1/workspaces/:wsId/docs */
/** Response from POST /api/v1/workspaces/:id/clone */
export interface CloneWorkspaceResponse {
  workspace: WorkspaceSummary;
  clonedBranches: number;
  clonedDrafts: number;
  /** How many chat sessions were carried into the fork (fork-chat; D31). */
  carriedSessions: number;
  /** The issue association the fork created, if it named an issue. */
  issueLink: ForkIssueLinkOutcomeDTO | null;
  /** The chat the fork started, if it asked for one. */
  startedChat?: ForkStartedChatDTO | null;
  /** Tracking-state advisories about the source at fork time. */
  sourceAdvisories?: ForkSourceAdvisory[];
  /** The re-verification run the fork started, when it asked for one. */
  reverificationRun?: ReverificationRun | null;
}

// ---------------------------------------------------------------------------
// 5b. Ontology Format Types (block-level tagging & validation)
// ---------------------------------------------------------------------------

/** Tag filter state for OntologyFormatPanel — checkboxes, not radio */
export interface TagFilter {
  human: boolean;
  uncertainty: boolean;
  /** When true, block tag background/border styling is suppressed */
  highlightOff: boolean;
}

export type ExecutionTargetPreference = "auto" | "cloud" | "local";

export interface WorkspaceMergeRunSummary {
  id: string;
  workspaceId: string;
  sourceKind?: string;
  sourceWorkspaceId?: string | null;
  targetWorkspaceId?: string | null;
  resolutionTaskId?: string | null;
  resolutionSessionId?: string | null;
  resolutionTaskStatus?: string | null;
  sourceBranchName: string;
  status: string;
  phase: string;
  blockingReason: string | null;
  /**
   * Per-repo child row this merge run resolves. Populated only for
   * `sourceKind === "remote_tracking"`.
   */
  repoId?: string;
  /**
   * `{ workspace_tip, mirror_tip }` snapshot at divergence detection
   * time. Populated only for `sourceKind === "remote_tracking"`.
   */
  sourceHeads?: {
    workspace_tip?: string;
    mirror_tip?: string;
  };
  /**
   * `{ baseline }` synchronization watermark. Populated only for
   * `sourceKind === "remote_tracking"`.
   */
  targetHeads?: {
    baseline?: string;
    refresh_reason?: string;
    previous_remote_tip?: string;
    supersedes_merge_run_id?: string;
  };
  /** ISO-8601 timestamp at run open. */
  startedAt?: string;
}

/** Response shape of `GET .../branches/:branchId/merge_runs`. */
export interface ListBranchMergeRunsResponse {
  items: WorkspaceMergeRunSummary[];
}

/** Response shape of `POST .../merge_runs/:mergeRunId/complete`. */
export interface CompleteBranchMergeRunResponse {
  completed: boolean;
}

/** Response shape of `POST .../merge_runs/:mergeRunId/start`. */
export interface StartBranchMergeRunResponse {
  run: WorkspaceMergeRunSummary;
}

export type MergePreflightDecision =
  | "fastForwardOrNoOp"
  | "cleanMergeNoGitConflicts"
  | "gitConflicts"
  | "workflowRequired";

export type MergePreflightRepoOutcome =
  | "identical"
  | "sourceAlreadyMerged"
  | "fastForward"
  | "cleanMerge"
  | "gitConflicts"
  | "workflowRequired";

export interface MergePreflightRepoResult {
  repoId: string;
  sourceWorkspaceBranchId: string;
  targetWorkspaceBranchId: string;
  sourceBranchName: string;
  targetBranchName: string;
  sourceHead: string;
  targetHead: string;
  outcome: MergePreflightRepoOutcome;
  conflictPaths: string[];
  message: string | null;
}

export interface MergePreflightResponse {
  sourceKind: "workspace" | string;
  sourceWorkspaceId: string | null;
  targetWorkspaceId: string | null;
  decision: MergePreflightDecision;
  canDirectMerge: boolean;
  preflightFingerprint: string;
  repoResults: MergePreflightRepoResult[];
  message: string;
}

export interface WorkspaceMergePreflightInput {
  targetWorkspaceId: string;
}

export interface StartWorkspaceMergeInput extends WorkspaceMergePreflightInput {
  agentLaunch: AgentLaunchSelectionV1;
  /**
   * Start and resolve with agent (g5.git.stopped-rebase.resolve-with-agent,
   * g5.chat.start-and-send): while the owner's VM is stopped, failed or
   * starting, the same request records (or joins) their Start consent and
   * queues the conflict-resolution brief bound to it.
   */
  startCompute?: boolean;
}

export interface DirectCleanWorkspaceMergeInput extends WorkspaceMergePreflightInput {
  preflightFingerprint: string;
}

export interface WorkspaceMergePreflightResponse {
  preflight: MergePreflightResponse;
}

/** One member of a source rebase the running workspace VM executed. */
export interface WorkspaceRebaseMember {
  repoId: string;
  previousHead: string;
  head: string;
  /** False when the member already contained the target (a no-op). */
  published: boolean;
}

export type StartWorkspaceMergeResponse =
  | { run: WorkspaceMergeRunSummary; resolution?: never; rebase?: never }
  | {
      run?: never;
      rebase?: never;
      resolution: { taskId: string; sessionId: string; status: string };
      /** The Start a `startCompute: true` request recorded or joined. */
      startCompute?: StartComputeOutcome;
    }
  | {
      run?: never;
      resolution?: never;
      /** The running VM rebased the workspace and G5 accepted the result. */
      rebase: {
        sourceWorkspaceId: string;
        targetWorkspaceId: string;
        vmId: string;
        members: WorkspaceRebaseMember[];
      };
    };

export type WorkspacePullRequestStatus = "open" | "merged" | "closed";
export type WorkspacePullRequestLocalMergeStatus =
  | "ready"
  | "conflict"
  | "stale"
  | "blocked"
  | "merged"
  | "closed";
export type WorkspacePullRequestProviderStatus =
  | "local_only"
  | "pending"
  | "open"
  | "action_required"
  | "failed"
  | "partial"
  | "merged";

export type WorkspacePullRequestProviderRepoStatus =
  | "pending_create"
  | "open"
  | "action_required"
  | "failed"
  | "merged";

export interface WorkspacePullRequestProviderRepoSummary {
  id: string;
  repoId: string;
  provider: "github" | string;
  providerRepoFullName: string;
  sourceRemoteBranch: string;
  targetRemoteBranch: string;
  providerPrNumber: number | null;
  providerPrUrl: string | null;
  providerStatus: WorkspacePullRequestProviderRepoStatus;
  sourceHeadSha: string | null;
  targetHeadSha: string | null;
  /** Exact admitted target after a G5 provider merge; older evidence is unknown. */
  mergeTargetHeadSha?: string | null;
  mergePending?: boolean;
  providerHeadSha: string | null;
  providerBaseSha: string | null;
  lastError: string | null;
  lastSyncedAt: string | null;
  updatedAt: string;
  /**
   * The provider merged this repository before G5 did (someone merged on
   * GitHub; g5.git.provider-merged-outside). Absent on older servers.
   */
  mergedOutsideG5?: boolean;
}

/**
 * One repository pull request of a G5 PR (g5.git.pr-repository): GitHub's for
 * a published member, or the local one, which is named by the G5 PR's number
 * (`PR3fa9c1`) and shares its lifecycle. An unchanged member has none.
 */
export interface RepositoryPullRequestSummary {
  repoId: string;
  /** GitHub's `owner/name`, or G5's repository name for a local one. */
  repository: string;
  adapter: "github" | "g5";
  /**
   * GitHub's number; null for a GitHub one not yet published and for a local
   * one, which the G5 PR's `number` names.
   */
  number: number | null;
  url: string | null;
  /** `OPEN` | `MERGED` | `CLOSED`; null for a GitHub one not yet published. */
  state: string | null;
  providerStatus?: string;
  /** GitHub's head/base branches, or the recorded G5 branches for a local one. */
  headRefName?: string | null;
  baseRefName?: string | null;
  headRefOid: string;
  baseRefOid: string;
  /**
   * The GitHub adapter's publication record, for a recorded GitHub one
   * (g5.git.pr-adapter). Surfaces read GitHub publication facts here.
   */
  github?: WorkspacePullRequestProviderRepoSummary;
}

/**
 * A G5 PR's recorded GitHub publications, from its GitHub repository pull
 * requests (g5.git.pr-adapter). Only a server that predates repository pull
 * requests sends just `providerRepos`, which is read the same way.
 */
export function githubPublications(
  pull: Pick<
    WorkspacePullRequestSummary,
    "repositoryPullRequests" | "providerRepos"
  >,
): WorkspacePullRequestProviderRepoSummary[] {
  if (pull.repositoryPullRequests) {
    return pull.repositoryPullRequests.flatMap((repository) =>
      repository.adapter === "github" && repository.github
        ? [repository.github]
        : [],
    );
  }
  return pull.providerRepos ?? [];
}

export interface WorkspacePullRequestSummary {
  id: string;
  /** Source owner's optional stop after automatic merge; defaults to off. */
  stopVmAfterMerge?: boolean;
  /**
   * Name written `PR` and six hex digits, e.g. `PR3fa9c1`, unique in the
   * organization, so it never reads as a GitHub number
   * (g5.git.pr-local-number); absent on older servers.
   */
  number?: string;
  sourceWorkspaceId: string;
  sourceWorkspaceName: string;
  targetWorkspaceId: string;
  targetWorkspaceName: string;
  status: WorkspacePullRequestStatus;
  localMergeStatus: WorkspacePullRequestLocalMergeStatus;
  providerStatus: WorkspacePullRequestProviderStatus;
  providerRepos?: WorkspacePullRequestProviderRepoSummary[];
  /** One per member the PR changes; absent on older servers. */
  repositoryPullRequests?: RepositoryPullRequestSummary[];
  /**
   * A deleted side keeps its PRs readable (g5.git.pr-reads,
   * g5.ui.deleted-workspace-prs); absent on older servers.
   */
  sourceWorkspaceDeleted?: boolean;
  targetWorkspaceDeleted?: boolean;
  title: string;
  descriptionOverride: string | null;
  sourceHeads: unknown;
  targetHeads: unknown;
  preflight: MergePreflightResponse;
  lastPreflightFingerprint: string;
  blockingMessage: string | null;
  createdAt: string;
  updatedAt: string;
  mergedAt: string | null;
  /**
   * Merge admission (g5.governance.approval.requirements): for an open PR
   * the target requirements against its recorded source cut, for a merged one
   * the admission it landed under. Absent on older servers.
   */
  mergeAdmission?: WorkspaceMergeAdmissionDTO | null;
  /**
   * Merge when ready (g5.git.pr-auto-merge), after gh's `autoMergeRequest`:
   * null while off. Absent on older servers.
   */
  autoMergeRequest?: WorkspacePullRequestAutoMergeRequest | null;
  /** User id of the person who merged it; absent on older servers. */
  mergedBy?: string | null;
  /**
   * The override this pull request was merged with
   * (g5.workspace.merge.admin-override.formal); non-null only for a merged
   * pull request that was. Absent on older servers.
   */
  mergeOverride?: MergeOverrideRecordDTO | null;
}

/** An unmet approval requirement a merge with override would waive. */
export interface MergeOverrideApprovalBlocker {
  definitionId: string;
  definitionName: string;
  definitionVersion: number;
  /** `pending` | `rejected`; strings are accepted for later states. */
  state: string;
}

/** One external check that blocks under the org's checks policy. */
export interface MergeOverrideCheck {
  name: string;
  /** `fail` | `cancel` | `pending` */
  bucket: string;
  required: boolean;
}

/** The blocking checks of one repository. */
export interface MergeOverrideCheckRepository {
  repository: string;
  headRefOid: string | null;
  /** `ready` | `pending` | `unavailable` */
  availability: string;
  checks: MergeOverrideCheck[];
}

/** A repository GitHub reports as blocked by its rules. */
export interface MergeOverrideGithubBlocker {
  repoId: string;
  repository: string;
  headSha: string;
  /** GitHub's merge state, e.g. `blocked`. */
  mergeState: string;
}

export interface MergeOverrideBlockers {
  approvals: MergeOverrideApprovalBlocker[];
  checks: MergeOverrideCheckRepository[];
  github: MergeOverrideGithubBlocker[];
}

export interface MergeOverrideSourceHead {
  repoId: string;
  branchName: string;
  workspaceId: string;
  headCommit: string;
}

/** Whether the caller may waive one group of blockers; decided by the server only. */
export interface MergeOverrideGroupAuthority {
  allowed: boolean;
  reason?: string | null;
}

/**
 * `GET …/pull-requests/:id/merge-override`: what a merge with override would
 * waive now, and who may waive it (g5.workspace.merge.admin-override.formal).
 */
export interface MergeOverridePreview {
  sourceHeads: MergeOverrideSourceHead[];
  blockers: MergeOverrideBlockers;
  /** Mechanical blockers (conflict, stale branch…) no override waives. */
  notOverridable: string[];
  authority: {
    g5: MergeOverrideGroupAuthority;
    github: MergeOverrideGroupAuthority;
  };
  /** The caller owns the source workspace. */
  selfOverride: boolean;
  /** Echoed by the merge so it refuses when the shown set changed. */
  digest: string;
}

export interface MergeWithOverrideInput {
  pullRequestId: string;
  reason: string;
  digest: string;
}

/** The recorded override of a merged pull request. */
export interface MergeOverrideRecordDTO {
  actorId: string;
  actorName?: string | null;
  at: string;
  reason: string;
  selfOverride: boolean;
  waivedApprovals: MergeOverrideApprovalBlocker[];
  waivedChecks: MergeOverrideCheckRepository[];
  githubRepositories: MergeOverrideGithubBlocker[];
}

/** A person's decision that G5 merges the pull request once it is ready. */
export interface WorkspacePullRequestAutoMergeRequest {
  /** User id of the person G5 merges as. */
  enabledBy: string;
  enabledAt: string;
  mergeMethod: "REBASE";
  /** The latest reason it is still waiting. */
  reason: string | null;
  checkedAt: string | null;
}

export interface WorkspacePullRequestProviderOutcome {
  hasSkippedRepos: boolean;
  allReposSkipped: boolean;
  someReposSkipped: boolean;
}

export function isWorkspacePullRequestProviderRepoSkipped(
  repo: WorkspacePullRequestProviderRepoSummary,
): boolean {
  return (
    repo.providerStatus === "action_required" ||
    repo.providerStatus === "failed"
  );
}

export function workspacePullRequestProviderOutcome(
  pullRequest: WorkspacePullRequestSummary,
): WorkspacePullRequestProviderOutcome {
  const repos = githubPublications(pullRequest);
  if (repos.length === 0) {
    const hasSkippedRepos =
      pullRequest.providerStatus === "action_required" ||
      pullRequest.providerStatus === "failed" ||
      pullRequest.providerStatus === "partial";
    return {
      hasSkippedRepos,
      allReposSkipped:
        pullRequest.providerStatus === "action_required" ||
        pullRequest.providerStatus === "failed",
      someReposSkipped: pullRequest.providerStatus === "partial",
    };
  }

  const skippedCount = repos.filter(
    isWorkspacePullRequestProviderRepoSkipped,
  ).length;
  const completedCount = repos.filter(
    (repo) =>
      repo.providerStatus === "open" ||
      repo.providerStatus === "merged" ||
      repo.providerPrNumber != null,
  ).length;
  const hasSkippedRepos = skippedCount > 0;
  return {
    hasSkippedRepos,
    allReposSkipped: hasSkippedRepos && completedCount === 0,
    someReposSkipped: hasSkippedRepos && completedCount > 0,
  };
}

export interface CreateWorkspacePullRequestInput {
  targetWorkspaceId?: string;
  /** Explicit title for the G5 and provider PRs; omitted preserves existing titles. */
  title?: string;
  /** Optional user-authored PR description override; omitted keeps per-repo
   * commit-derived provider bodies (backend spec D15). */
  description?: string;
  /** `update` (the Update PR button, which saves first; `g5d pr update`
   * republishes without saving) requires an open G5 PR and never creates one;
   * `create` (default) reuses or opens one. */
  mode?: "create" | "update";
}

export interface PreviewWorkspacePullRequestTextInput {
  targetWorkspaceId: string;
}

/** Commit-derived PR text preview (`POST …/pull-requests/preview`);
 * `derived = false` carries the legacy workspace-name title (the body may
 * still come from the Project Doc). `source` (backend spec D16) attributes
 * the body: the workspace PR-body Project Doc
 * (`docs/<workspace_id>/<workspace-name-slug>.md` in the docs repo),
 * commit-derived content, or the legacy fallback. Optional for
 * rolling-upgrade tolerance. */
export interface PreviewWorkspacePullRequestTextResponse {
  title: string;
  body: string;
  derived: boolean;
  source?: "projectDoc" | "derived" | "legacy";
}

export interface WorkspaceDirtyDraftSummary {
  workspaceId: string;
  workspaceRole: "source" | "target";
  repoId: string;
  repoName: string;
  branchName: string;
  dirtyCount: number;
}

export interface CreateWorkspacePullRequestResponse {
  pullRequest: WorkspacePullRequestSummary;
  alreadyExisted: boolean;
  dirtyDrafts: WorkspaceDirtyDraftSummary[];
  /** What this request did to each provider repository. */
  publication?: PullRequestPublication;
}

export interface PullRequestPublication {
  /** Every provider PR this request synchronized is open with the saved head. */
  complete: boolean;
  repositories: Array<{
    repository: string;
    number: number | null;
    url: string | null;
    /** created, reused, replaced, action_required, failed, merged or unchanged */
    action: string;
    head: "published" | "pending" | null;
    title: "applied" | "not_applied" | null;
    body: "applied" | "not_applied" | null;
    message: string | null;
  }>;
}

/** A provider PR's lifecycle as GitHub reports it. */
export type ProviderPullRequestLifecycle =
  | "open"
  | "draft"
  | "closed"
  | "merged";

/**
 * `GET …/pull-requests/:id/description` (git g5.git.pr-description): the G5
 * PR's canonical description and how each provider publication compares
 * with what G5 would publish there. A read; nothing is stored or written.
 */
export interface PullRequestDescriptionDTO {
  pullRequestId: string;
  /** The saved description, else the PR-body Project Doc; `null` for neither. */
  description: string | null;
  descriptionSource: "saved" | "projectDoc" | null;
  /** GitHub's rendering of `description`; `null` when not rendered. */
  descriptionHtml: string | null;
  repositories: ProviderPullRequestDescriptionDTO[];
}

export interface ProviderPullRequestDescriptionDTO {
  repoId: string;
  repository: string;
  number: number;
  url: string | null;
  /** `null` when GitHub could not be read. */
  lifecycle: ProviderPullRequestLifecycle | null;
  comparison: "matches" | "differs" | "unavailable";
  body: string | null;
  bodyHtml: string | null;
  error: string | null;
}

export interface MergeWorkspacePullRequestInput {
  pullRequestId: string;
}

export interface MergeWorkspacePullRequestResponse {
  pullRequest: WorkspacePullRequestSummary;
  run: WorkspaceMergeRunSummary;
}

export interface WorkspacePullRequestActionInput {
  pullRequestId: string;
}

export interface WorkspacePullRequestActionResponse {
  pullRequest: WorkspacePullRequestSummary;
}

/** Spec context embedded in TaskDTO */
export interface SpecContextDTO {
  componentId: string;
  srcFiles: string[];
  parentSpec: string | null;
  childSpecs: string[];
}

export type TaskLifecyclePosture =
  | "active"
  | "resumable"
  | "action_required"
  | "terminal";

/** Canonical logical task states persisted by the workspace task lifecycle. */
export type TaskStatus =
  | "queued"
  | "running"
  | "waiting"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface TaskExecutionAttemptDTO {
  id: string;
  attemptNumber: number;
  provider: string | null;
  providerSessionId: string | null;
  disposition: string | null;
  reason: string | null;
  retryPolicy: string | null;
  lifecyclePosture: TaskLifecyclePosture;
  startedAt: string;
  endedAt: string | null;
  continuationSummary?: string | null;
}

/** Task DTO from GET /api/v1/tasks/:id (also used in task task listings) */
export interface TaskDTO {
  id: string;
  name: string;
  taskType: string; // 'bootstrap' | 'code' | 'test' etc.
  status: TaskStatus;
  workspaceId: string;
  workspaceName: string;
  specContext: SpecContextDTO | null;
  dependsOn: string[];
  filesChanged: string[];
  costUsd: number;
  sessionId: string | null;
  assignedAt: string | null;
  completedAt: string | null;
  /** Server-derived semantic state; never infer recoverability from raw status. */
  lifecyclePosture?: TaskLifecyclePosture;
  /** Latest fenced execution attempt; full history is cursor-paginated. */
  latestAttempt?: TaskExecutionAttemptDTO | null;
  /**
   * Claude's live background set for the running attempt: the raw latest
   * `background_tasks.replaced` payload, present only while non-empty and only
   * on the session's live-task view.
   */
  backgroundWait?: unknown;
  nativeSteeringAvailable?: boolean;
}

/** Convenience alias — TaskDTO is used for both summary and detail */
export type TaskSummary = TaskDTO;
export type TaskDetail = TaskDTO;

/** Agent timeline entry — execution trace */
export interface TimelineEntry {
  id: string;
  entryType: string; // 'thought' | 'tool_call' | 'tool_result' | 'output' | 'error' etc.
  content: string;
  metadata: Record<string, unknown>;
  sequence?: number;
  createdAt: string;
}

/** A single commit on a workspace branch (GET /api/v1/branches/:branchId/commits). */
export interface CommitSummary {
  /** Full 40-character hex commit SHA. */
  sha: string;
  /** First 7 characters of the SHA, for compact display. */
  shortSha: string;
  /** First line of the commit message. */
  summary: string;
  /** Commit author name. */
  authorName: string;
  /** Committer time as an RFC 3339 / ISO 8601 timestamp. */
  committedAt: string;
}

/** One entry in a repository's full first-parent Source Control history. */
export interface CommitHistoryEntry extends CommitSummary {
  parentCount: number;
}

export interface CommitHistoryPage {
  commits: CommitHistoryEntry[];
  nextCursor: string | null;
}

export type CommitFileStatus = "added" | "modified" | "deleted" | "renamed";

export interface CommitDiffFileSummary {
  status: CommitFileStatus;
  oldPath: string | null;
  newPath: string | null;
  additions: number;
  deletions: number;
}

export type CommitComparisonBase =
  | { kind: "parent"; sha: string }
  | { kind: "emptyTree" };

export interface CommitDiffResponse {
  commit: CommitHistoryEntry;
  comparisonBase: CommitComparisonBase;
  firstParentDiff: boolean;
  files: CommitDiffFileSummary[];
  totalAdditions: number;
  totalDeletions: number;
}

export interface CommitFileDiffResponse {
  path: string;
  status: "text" | "binary" | "tooLarge";
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

/**
 * A named preview profile (GET /api/v1/branches/:branchId/preview-profiles).
 * Each maps to a committed `.g5/preview_profiles/<name>.yaml`.
 */
export interface PreviewProfile {
  /** Profile name (the file stem). */
  name: string;
  /** Shell command that launches the app. */
  command: string;
  /** TCP port the app listens on inside the guest. */
  port: number;
  /** Working directory for the command, relative to /work unless absolute. */
  workingDir: string | null;
  /** Optional human description for the picker. */
  description: string | null;
  /** Names of the extra env vars the profile sets (values omitted). */
  envKeys: string[];
  /** Shell command the guest runs after the app stops (Close or self-exit). */
  cleanupCommand: string | null;
}

/** One valid committed Preview application configuration on one branch/ref. */
export interface PreviewApplication {
  source: "workspaceDefault" | "namedProfile";
  /** Null for `.g5/workspace.yaml`; otherwise the named profile stem. */
  profile: string | null;
  /** Exact repo-relative committed config file. */
  configPath: string;
  command: string;
  port: number;
  workingDir: string | null;
  description: string | null;
  envKeys: string[];
  cleanupCommand: string | null;
}

/**
 * The Start a `startCompute: true` preview start recorded
 * (g5.preview.start-and-preview): the preview task is bound to it and runs
 * once that Start brings the VM up.
 */
export interface StartPreviewOutcome {
  startCompute?: StartComputeOutcome;
}

/** Request body for POST /api/v1/branches/:branchId/preview-profiles */
export interface CreatePreviewProfileInput {
  name: string;
  command: string;
  port: number;
  workingDir?: string;
  env?: Record<string, string>;
  description?: string;
  /** Optional shell command run after the app stops. */
  cleanupCommand?: string;
  /** CAS guard: the branch head the client based this create on. */
  expectedHeadCommit?: string;
  /** When true, overwrite an existing profile of the same name. */
  overwrite?: boolean;
}

/** Response from POST /api/v1/branches/:branchId/preview-profiles */
export interface CommitPreviewProfileResult {
  /** The branch's new head commit after the profile was committed. */
  headCommit: string;
  /** The created profile. */
  profile: PreviewProfile;
}

export interface SyncAgentConfig {
  agentProvider: ChatAgentProvider;
  /** Whether this surface inherits Runtime Config, uses canonical defaults, or
   * explicitly pins the visible controls for this run. */
  launchSource?: "inherit_configured" | "canonical_default" | "explicit";
  model?: string;
  modelLabel?: string;
  permissionMode?: string;
  permissionLabel?: string;
  effort?: string;
}

export type AgentLaunchFieldPreferenceV1<T> =
  | { source: "inherit_configured" }
  | { source: "canonical_default" }
  | { source: "explicit"; value: T };

/** Closed provider-account kind catalog (g5-common/types/core D36). */
export type ProviderAccountKind =
  | "claude_subscription"
  | "anthropic_api_key"
  | "anthropic_compatible_bearer"
  | "openrouter_api_key"
  | "openai_api_key"
  | "chatgpt_subscription";

export type ProviderAccountScope = "org" | "workspace" | "user";

/**
 * Per-account subscription usage snapshot carried on the accounts list
 * (settings-page D18; provider quota snapshot D13). Populated only for
 * `claude_subscription` accounts in v1 — kinds without a passive header source
 * carry `null`. Display-only: it never gates dispatch.
 */
export interface ProviderAccountUsage {
  fiveHourUtilization: number | null;
  fiveHourResetAt: string | null;
  sevenDayUtilization: number | null;
  sevenDayResetAt: string | null;
  capturedAt: string;
}

/**
 * Named provider account row (external_agent_provider_contract D14). The
 * credential is write-only: `secretHint` is the server's bounded
 * beginning-of-key hint (secret_custody_and_redaction D9), never the value.
 * `usage` is the account-keyed snapshot (settings-page D18), `null` for kinds
 * without a passive header source.
 */
export interface ProviderAccountDTO {
  id: string;
  orgId: string;
  name: string;
  kind: ProviderAccountKind;
  family: "claude" | "codex" | "both";
  scope: ProviderAccountScope;
  workspaceId: string | null;
  userId: string | null;
  baseUrl: string | null;
  hasSecret: boolean;
  secretHint: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  usage: ProviderAccountUsage | null;
}

/** `POST /api/v1/orgs/:orgId/provider-accounts` body. */
export interface CreateProviderAccountInput {
  name: string;
  kind: ProviderAccountKind;
  scope: ProviderAccountScope;
  workspaceId?: string;
  baseUrl?: string;
  secret?: string;
}

/**
 * `PATCH /api/v1/orgs/:orgId/provider-accounts/:id` body: only changed
 * fields; a blank/absent `secret` keeps the stored credential; kind and scope
 * are immutable and have no field here.
 */
export interface UpdateProviderAccountInput {
  name?: string;
  baseUrl?: string;
  secret?: string;
}

/** Explicit value of the `account` launch preference. */
export type AgentAccountSelectionV1 =
  | { kind: "account"; accountId: string }
  | { kind: "runtime_config" };

export interface AgentLaunchPreferencesV1 {
  schemaVersion: 1;
  provider: AgentLaunchFieldPreferenceV1<ChatAgentProvider>;
  model: AgentLaunchFieldPreferenceV1<string>;
  permissionMode: AgentLaunchFieldPreferenceV1<string>;
  effort: AgentLaunchFieldPreferenceV1<string>;
  /** Additive; absent means `inherit_configured` (the effective assignment). */
  account?: AgentLaunchFieldPreferenceV1<AgentAccountSelectionV1>;
}

export interface ResolvedAgentLaunchFieldV1<T> {
  value: T;
  provenance: "explicit" | "configured" | "canonical_default";
  configKey?: string;
}

export type AgentAuthModeV1 =
  | "no_credential"
  | "open_router_api_key"
  | "open_ai_api_key"
  | "chat_gpt_subscription"
  | "anthropic_api_key"
  | "anthropic_bearer"
  | "anthropic_oauth";

export type AgentEgressModeV1 =
  | "no_network"
  | "host_llm_proxy" // Historical stored launches only.
  | "direct_provider";

/** Live display usage. Utilizations are fractions; timestamps are RFC3339. */
export interface SubscriptionUsage {
  fiveHourUtilization?: number | null;
  fiveHourResetAt?: string | null;
  sevenDayUtilization?: number | null;
  sevenDayResetAt?: string | null;
  representativeClaim?: string | null;
  unifiedStatus?: string | null;
  capturedAt: string;
}

/** Live-only observation bound to the attempt's account and auth route. */
export interface LiveSubscriptionUsage {
  authMode: AgentAuthModeV1;
  accountId: string | null;
  usage: SubscriptionUsage;
}

export type AgentProviderRouteV1 =
  | "none"
  | "openrouter"
  | "openai_direct"
  | "anthropic_direct"
  | "compatible_endpoint";

export interface ResolvedAgentExecutionV1 {
  schemaVersion: 1;
  resolutionId: string;
  configRevision: string;
  catalogVersion: string;
  provider: ResolvedAgentLaunchFieldV1<ChatAgentProvider>;
  providerRoute: AgentProviderRouteV1;
  model: ResolvedAgentLaunchFieldV1<string>;
  upstreamModel: string;
  permissionMode: ResolvedAgentLaunchFieldV1<string>;
  effort: ResolvedAgentLaunchFieldV1<string>;
  authMode: AgentAuthModeV1;
  egressMode: AgentEgressModeV1;
  credentialReady: boolean;
  requiresInteractiveAuth: boolean;
  /** Frozen account triple, present only when an account was selected. */
  accountId?: string;
  accountKind?: ProviderAccountKind;
  accountScope?: ProviderAccountScope;
}

export interface AgentLaunchSettingsPreviewV1 {
  scope: "org" | "workspace" | "user";
  settings?: Record<string, unknown>;
  removeSettingKeys?: string[];
}

export interface ResolveAgentLaunchRequestV1 {
  preferences: AgentLaunchPreferencesV1;
  settingsPreview?: AgentLaunchSettingsPreviewV1[];
}

export interface AgentLaunchSelectionV1 {
  preferences: AgentLaunchPreferencesV1;
  confirmedResolutionId: string;
}

export const configuredAgentLaunchPreferences =
  (): AgentLaunchPreferencesV1 => ({
    schemaVersion: 1,
    provider: { source: "inherit_configured" },
    model: { source: "inherit_configured" },
    permissionMode: { source: "inherit_configured" },
    effort: { source: "inherit_configured" },
  });

export const canonicalAgentLaunchPreferences =
  (): AgentLaunchPreferencesV1 => ({
    schemaVersion: 1,
    provider: { source: "canonical_default" },
    model: { source: "canonical_default" },
    permissionMode: { source: "canonical_default" },
    effort: { source: "canonical_default" },
  });

export interface SyncContinuationStateDTO {
  taskId: string;
  taskStatus: string;
  waitingForChatInput: boolean;
  canFinish: boolean;
}

// ---------------------------------------------------------------------------
// 7. Chat Types
// ---------------------------------------------------------------------------

/** Chat session list item from GET /api/v1/workspaces/:id/sessions */
export interface ChatSessionDTO {
  id: string;
  workspaceId: string;
  /**
   * Persona the session started with (workbench g5.workbench.persona);
   * `null`/absent for sessions started outside the workbench or on an
   * older server.
   */
  persona?: WorkspacePhase | null;
  scopeType: string;
  scopeRef: string | null;
  model: string | null;
  agentProvider: ChatAgentProvider;
  providerState?: Record<string, unknown> | null;
  title: string | null;
  summary: string | null;
  tags: string[];
  messageCount: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCostUsd: number;
  forkedFromId: string | null;
  linkedCommits: string[];
  createdAt: string;
  updatedAt: string;
  canSend: boolean;
  /**
   * Frozen launch auth mode for this session, sourced server-side from the
   * session's persisted launch resolution (external_agent_provider_contract
   * D11 — never re-resolved on the client). Present on the session-detail
   * read; additive-optional (absent on old servers / list items / sessions
   * with no persisted launch yet).
   */
  authMode?: AgentAuthModeV1 | null;
  /**
   * Live observation joined by useChatSession from browser memory, only for
   * the same subscription auth route and account. Never read from history.
   */
  subscriptionUsage?: SubscriptionUsage | null;
  /**
   * Frozen provider account of the session's latest persisted launch
   * (routes/chat D31): id, kind, and scope only — never the name or any
   * secret-derived field, never re-resolved on the client. Absent/null when
   * the session has no persisted launch or its launch names no account.
   */
  account?: {
    id: string;
    kind: ProviderAccountKind;
    scope: ProviderAccountScope;
  } | null;
  /**
   * Committed next-turn account default stored under the active provider's
   * `provider_state.<namespace>.settings` (external_agent_provider_contract
   * D12; right-panel session_provider_controls D30). Session start seeds it
   * from the first launch's resolved account, so it is non-null for every
   * session started through the launch resolver; a later change applies only
   * to turns allocated after it committed. `null`/absent occurs only for
   * legacy sessions (the chat field then falls back to `account`).
   */
  nextAccount?: AgentAccountSelectionV1 | null;
  /**
   * Turn activity and the caller's inspection state (chat g5.chat.activity),
   * derived server-side from task and attempt records. Present on the list
   * and detail reads; absent on search results, mutation responses and
   * older servers.
   */
  activity?: ChatSessionActivityDTO;
}

/** One finished result of a chat turn (chat g5.chat.activity.turns). */
export interface ChatSessionResultDTO {
  taskId: string;
  /** `null` for a turn that ended without any attempt. */
  attempt: number | null;
  /** The attempt recorded assistant output the transcript can display. */
  hasOutput: boolean;
}

/** A chat's turn activity as the signed-in person sees it. */
export interface ChatSessionActivityDTO {
  /** When the active turn started; `null` when no turn is active. */
  activeTurnStartedAt: string | null;
  /** When the latest finished result ended; `null` before the first. */
  lastFinishedAt: string | null;
  latestResult: ChatSessionResultDTO | null;
  /** The latest result is not yet inspected by this person. */
  unseenResult: boolean;
  /**
   * The turn waits for the person: a pending question or permission request
   * of its running attempt, or a deferred question holding it (chat
   * g5.chat.activity.awaiting-input). Absent on older servers.
   */
  awaitingInput?: boolean;
}

/** Provider choices held in the composer and submitted with the next turn. */
export type ProviderControlSetSettingsPayload = {
  model: string;
  effort: string;
  account?: AgentAccountSelectionV1;
};

/** Public terminal login identity; never contains a device code or credentials. */
export interface CodexAuthFailure {
  taskId: string;
  attempt: number;
}

/** Chat message DTO from GET /api/v1/sessions/:id/messages */
export interface ChatMessageDTO {
  id: string;
  sessionId: string;
  role: string; // 'user' | 'assistant' | 'system' | 'tool'
  content: string;
  artifacts: Artifact[] | null;
  citations: Citation[] | null;
  inputTokens: number | null;
  outputTokens: number | null;
  status: string; // 'pending' | 'streaming' | 'complete' | 'error' | 'stopped'
  messageIndex: number;
  createdAt: string;
  /** The execution attempt that produced the message, when it has one
   * (chat g5.chat.inspection.acknowledge). Absent on older servers. */
  taskId?: string | null;
  attempt?: number | null;
  /** A failed login before delivery, persisted by this exact task/attempt. */
  codexAuthFailure?: CodexAuthFailure;
  /** Content is a platform-authored error explanation, not partial agent output.
   * Optional while older API pods drain. */
  isErrorNotice?: boolean;
  /**
   * Client-only diagnostic error string for an errored message, populated from
   * the `chat.error` WebSocket event's human-readable `message` field
   * (websocket D15). Not part of the server DTO — a server refetch does not
   * carry it, so it is transient until the cache reconverges.
   */
  errorMessage?: string | null;
}

/**
 * What a Stop did (chat g5.chat.controls): the runtime's disposition for a
 * running turn, how a queued/held turn ended, or that the committed intent is
 * still being delivered. Older servers answer `stopped` alone.
 */
export type ChatStopDisposition =
  | "signalling"
  | "already_finished"
  | "cancelled"
  | "ended"
  | "withdrawn"
  | "already_ended"
  | "undelivered";

export interface StopGenerationResponseDTO {
  stopped: boolean;
  disposition?: ChatStopDisposition | string;
}

export type LiveTailResponseState = "none" | "live" | "catching_up";

/**
 * The running turn's output the API has fetched but not yet saved
 * (chat g5.chat.live-tail-recovery): rows with sequence above
 * `persistedThrough`, complete from the saved prefix when `state` is `live`.
 */
export interface LiveTailResponseDTO {
  state: LiveTailResponseState;
  taskId?: string | null;
  attempt?: number | null;
  persistedThrough?: number | null;
  fetchedThrough?: number | null;
  entries: TimelineEntry[];
}

export interface SendMessageResponseDTO {
  messageId: string;
  status: string;
  /** Additive in issue #1050; absent while an older API pod drains. */
  createdAt?: string;
  inputId?: string;
  taskId?: string;
  delivery?: SessionInputDelivery;
  /**
   * Present when the send carried `startCompute: true`: the Start it
   * recorded, to which the queued input is bound (g5.personal.start-and-send).
   */
  startCompute?: StartComputeOutcome;
}

export type SessionInputDelivery =
  | "queued"
  | "reserved"
  | "delivered"
  | "delivery_unknown"
  | "withdrawn";

export interface SessionInputDTO {
  id: string;
  sessionId: string;
  inputIndex: number;
  content: string;
  artifacts: unknown;
  revision: number;
  inputOrigin: "human" | "workflow" | "continuation";
  delivery: SessionInputDelivery;
  deliveryGeneration: number;
  taskId?: string | null;
  attempt?: number | null;
  createdAt: string;
  deliveredAt?: string | null;
  withdrawnAt?: string | null;
  /**
   * Server-evaluated permissions for the queued-input controls (issue #629);
   * absent while an older API pod drains. They fold in state the other
   * fields cannot show: a steering reservation and a frozen
   * possibly-delivered revision after authorized redelivery.
   */
  editable?: boolean;
  withdrawable?: boolean;
}

export interface ChatProviderTurnOptions {
  model?: string;
  permissionMode?: string;
  effort?: string;
  /**
   * The account for this turn (routes/chat D33). Omitted, the chat's
   * saved next-turn account applies; once accepted, the choice is saved.
   */
  account?: AgentAccountSelectionV1;
}

export type ChatProjectReferenceKind = "project_doc" | "view" | "source_file";

export interface ChatProjectReference {
  id: string;
  kind: ChatProjectReferenceKind;
  label: string;
  path?: string;
  docId?: string;
  branchId?: string;
}

/**
 * An externally uploaded file (e.g. a screenshot) attached to a chat turn.
 * Unlike a {@link ChatProjectReference}, which only points at an existing
 * workspace path, an upload carries the file bytes inline: `contentBase64`
 * is the RAW base64 payload (no `data:` URL prefix). Sent inline in the
 * send/start request `attachments` array; the backend persists it and
 * materializes the file into the agent worktree for the same turn.
 */
export interface ChatUploadAttachment {
  fileName: string;
  contentType?: string;
  contentBase64: string;
}

export interface ChatMessageContext {
  fileRanges?: Array<{
    path: string;
    startLine: number;
    endLine: number;
  }>;
  taskId?: string;
  activeFilePath?: string;
  openFilePaths?: string[];
  projectReferences?: ChatProjectReference[];
}

export interface NativeProviderRequestReplyResponseDTO {
  taskId: string;
  attempt: number;
  providerRequestId: string;
  requestId: string;
  responseDigest: string;
  deliveryDisposition:
    | "delivered"
    | "already_delivered"
    | "queued"
    /** An accepted answer whose request ended before delivery; it is never
     * newly queued (`g5.chat.native-question-recovery`). */
    | "undeliverable";
  /** The Start a `startCompute: true` reply recorded (g5.chat.start-and-send). */
  startCompute?: StartComputeOutcome;
}

/** UI decision plus the execution identity required by the native reply API. */
export interface AgentApprovalResolutionInput {
  sessionId: string;
  approvalId: string;
  decision: "approved" | "rejected";
  reason?: string;
  updatedInput?: unknown;
  updatedPermissions?: unknown;
  interrupt?: boolean;
  taskId?: string;
  attempt?: number;
  providerRequestId?: string;
  requestKind?: "question" | "permission";
  validatedResponse?: Record<string, unknown>;
  /**
   * Start and answer (g5.chat.start-and-send): record Start consent with
   * the answer, so the held turn resumes once the VM runs.
   */
  startCompute?: boolean;
}

export interface StartChatSessionResponseDTO {
  session: ChatSessionDTO;
  message: SendMessageResponseDTO;
  assistantMessageId: string;
  taskId: string;
  /** The Start a `startCompute: true` start recorded (g5.personal.start-and-send). */
  startCompute?: StartComputeOutcome;
}

/** Artifact embedded in chat messages */
export interface Artifact {
  type: ArtifactType;
  title?: string;
  content: string | object; // String for text-based, object for structured (chart, dataframe)
  language?: string;
  filePath?: string;
  actions?: ArtifactAction[];
}

/** Artifact type enumeration */
export type ArtifactType =
  | "code" // Syntax highlighted code block → Monaco (read-only)
  | "diff" // Unified diff → Monaco diff viewer
  | "table" // Markdown or JSON table → custom table component
  | "chart" // Chart.js specification → react-chartjs-2
  | "diagram" // Mermaid diagram → MermaidDiagram component
  | "image" // Base64 or URL image → <img> with lightbox
  | "file" // Downloadable file content → download button + preview
  | "terminal" // Command + output → terminal-styled block
  | "math" // LaTeX equation → KaTeX
  | "json" // Formatted JSON with folding → react-json-view
  | "dataframe"; // Tabular data with sorting/filtering → AG Grid or similar

/** Action button on an artifact */
export interface ArtifactAction {
  label: string;
  icon: string;
  action:
    | { type: "copy" }
    | { type: "download"; filename: string }
    | { type: "apply"; targetFile: string } // Apply diff/code to file
    | { type: "run"; command: string }; // Execute in terminal
}

/** Citation reference in chat messages (server `ChatCitation` shape).
 *  `repoId` names the workspace repo that owns `filePath` (chat_service D55,
 *  approved Q2b); absent on legacy rows — the client then opens against the
 *  active branch unchanged. The never-emitted `commitSha` drift field was
 *  removed with the same-PR contract test (risk R8). */
export interface Citation {
  filePath?: string;
  repoId?: string;
  lineRange?: [number, number];
  label: string;
}

// ---------------------------------------------------------------------------
// 8. Comment Types
// ---------------------------------------------------------------------------

/** Comment DTO from GET /api/v1/workspaces/:id/comments */
export interface CommentDTO {
  id: string;
  workspaceId: string;
  authorId: string;
  authorName: string; // flat — not a nested UserRef
  authorAvatarUrl: string | null;
  scope: string;
  repoId: string | null;
  filePath: string | null;
  lineNumber: number | null;
  /** Absent only on historical comments created before hash anchors existed. */
  lineHash?: string | null;
  contentHash?: string | null;
  anchorContext?: string | null;
  commitSha: string | null;
  content: string;
  resolved: boolean;
  resolvedAt: string | null;
  originBranch: string;
  replyCount: number;
  createdAt: string;
  updatedAt: string;
}

/** Comment reply DTO from GET /api/v1/comments/:id/replies */
export interface CommentReplyDTO {
  id: string;
  commentId: string;
  authorId: string;
  authorName: string;
  authorAvatarUrl: string | null;
  content: string;
  createdAt: string;
}

/**
 * What the viewer may do in a file's anchored threads
 * (`g5.documents.comments.authority`).
 */
export interface CommentPermissions {
  /** Start a thread: anyone who can read a writable workspace. */
  canStartThread: boolean;
  /** Reply: anyone who can read a writable workspace. */
  canReply: boolean;
  /** Resolve, reopen and delete any thread: the current owner. */
  canModerate: boolean;
}

/** Nothing writable: the read-only default when permissions are unknown. */
export const READ_ONLY_COMMENT_PERMISSIONS: CommentPermissions = {
  canStartThread: false,
  canReply: false,
  canModerate: false,
};

/** Request body for POST /api/v1/workspaces/:workspaceId/comments */
export interface CreateCommentInput {
  /** Optional caller-owned identity for an idempotent retry. */
  id?: string;
  /** New comments are line comments; node-level comments are retired. */
  scope: "line";
  content: string;
  repoId: string;
  filePath: string;
  lineNumber: number;
  /** SHA-256 of the exact UTF-8 bytes on the anchored line. */
  lineHash?: string;
  /** SHA-256 of the exact displayed file bytes. */
  contentHash?: string;
  /** Bounded surrounding source used for unambiguous remapping. */
  anchorContext?: string;
  commitSha?: string;
  /**
   * A label only: it never selects a thread. Omitted, the server records the
   * workspace's branch (`g5.documents.comments.identity`).
   */
  originBranch?: string;
  /** Optional: explicit @mention user IDs resolved by the mention picker */
  mentionUserIds?: string[];
}

// ---------------------------------------------------------------------------
// 10. Settings Types
// ---------------------------------------------------------------------------

/** Workspace settings from GET /api/v1/workspaces/:workspaceId/settings */
export interface WorkspaceSettings {
  workspaceId: string;
  maxSpendUsd: number | null; // null = unlimited
  spendPeriod: string; // 'hourly' | 'daily' | 'weekly' | 'monthly' | 'total'
  spendPeriodStart: string | null;
  maxInputTokens: number | null;
  maxOutputTokens: number | null;
  idleTimeoutSecs: number;
  responseDeadlineSecs: number;
  boundedExecutionSlots: number;
  permissionPolicy: Record<string, unknown> | null;
  /** Owner opt-in for this workspace's agent to change other workspaces. */
  agentMayModifyOtherWorkspaces: boolean;
  updatedBy: string | null; // user ID string, not UserRef
  updatedAt: string;
}

/** Hierarchical runtime config from org/workspace/user config endpoints */
export interface ScopedConfig {
  scope: "org" | "workspace" | "user";
  settings: Record<string, unknown>;
  secretKeys: string[];
  /** Bounded beginning-of-key display hints for this scope's own secrets
   * (config route D6; security_policy secret_custody_and_redaction D9):
   * key → first 12 characters of the stored value, present only for
   * hint-eligible values. Server-derived display metadata — the client never
   * derives hints itself (settings-page D9). Optional for rolling-deploy
   * tolerance against older servers. */
  secretKeyHints?: Record<string, string>;
  effectiveSettings: Record<string, unknown>;
  effectiveSecretKeys: string[];
  /** Bounded display hints for the effective/winning secret values (config
   * route D6); same eligibility and audience rules as `secretKeyHints`. */
  effectiveSecretKeyHints?: Record<string, string>;
  /** Compatibility capability marker; always true in G5 Lean. */
  forkChatEnabled?: boolean;
  updatedBy: string | null;
  updatedAt: string | null;
}

/** Request payload for org/workspace/user config updates */
export interface UpdateScopedConfigInput {
  /**
   * Partial `settings` merge patch: present keys are merged into the scope's
   * stored settings and unspecified keys are preserved (the server does NOT
   * wholesale-replace). Deleting a key requires `removeSettingKeys`.
   */
  settings?: Record<string, unknown>;
  /** Top-level settings keys to delete after the merge (mirrors `removeSecretKeys`). */
  removeSettingKeys?: string[];
  setSecrets?: Record<string, string>;
  removeSecretKeys?: string[];
}

/**
 * Left-sidebar display mode (left-sidebar D38; ui g5.ui.sidebar-default):
 * `expanded` is the always-on stacked panel, `collapse-to-rail` (default) is
 * that panel whose collapsed state is the icon rail with an expand control
 * (an icon press expands the panel on its section), `expand-on-hover` is the
 * icon activity rail that opens the stacked panel on hover/focus, and `rail`
 * opens it only on explicit icon activation.
 */
export type SidebarDisplayMode =
  | "expanded"
  | "collapse-to-rail"
  | "expand-on-hover"
  | "rail";

/**
 * Phase bar rendering under the alignment layout (workbench
 * g5.workbench.header-styles): today's chevrons with the deliverable subline,
 * the same without it, or a column of short chevrons beside the action bar.
 */
export type PhaseBarStyle = "extended" | "minimal" | "vertical";

/**
 * Action bar rendering under the alignment layout (workbench
 * g5.workbench.header-styles): glyph-over-label columns, icon-only buttons,
 * captioned text buttons with a leading icon, one row of icon+label, or
 * `dynamic` — glyph-over-label columns that shrink one step, then scroll.
 */
export type ActionBarStyle =
  | "extended-icons"
  | "minimal-icons"
  | "extended-text"
  | "simplified"
  | "dynamic";

/**
 * What the ribbon actually renders: a fixed style, or the compact column the
 * `dynamic` setting shrinks to when the row stops fitting.
 */
export type RibbonStyle = Exclude<ActionBarStyle, "dynamic"> | "compact-icons";

/**
 * Per-user composer subscription-usage indicator window
 * (agent_interaction_ux D8): `7d` (default) and `5h` select which rate-limit
 * window the read-only usage chip headlines; `off` hides the chip everywhere,
 * for both Claude and Codex subscription sessions. Unknown stored values
 * fall back to `7d`.
 */
export type SubscriptionUsageWindowSetting = "7d" | "5h" | "off";

/**
 * Faces of the chat bubble stack above the Chat corner button (ui
 * g5.workspace.ui.chat-bubble-stack): `off` keeps the button alone, `still`
 * one image per state, `animated` alternates each state's two frames, `3d`
 * (default) draws a robot-heads head. Unknown stored values fall back to
 * `3d`.
 */
export type ChatBubbleStyle = "off" | "still" | "animated" | "3d";
export const CHAT_BUBBLE_STYLES: readonly ChatBubbleStyle[] = [
  "off",
  "still",
  "animated",
  "3d",
];
export function isChatBubbleStyle(value: unknown): value is ChatBubbleStyle {
  return (CHAT_BUBBLE_STYLES as readonly unknown[]).includes(value);
}

/**
 * How many bubbles the chat bubble stack shows before the rest fold into a
 * count (ui g5.workspace.ui.chat-bubble-stack): a whole number from 1 to 50,
 * the list's page size. Default 5; other stored values fall back to it.
 */
export const CHAT_BUBBLE_LIMIT_MIN = 1;
export const CHAT_BUBBLE_LIMIT_MAX = 50;
export const CHAT_BUBBLE_LIMIT_DEFAULT = 5;
export function isChatBubbleLimit(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= CHAT_BUBBLE_LIMIT_MIN &&
    value <= CHAT_BUBBLE_LIMIT_MAX
  );
}

/** Page size of the organization Home tiles (workbench g5.workbench.home-workspaces). */
export type HomeRowsPerTile = 5 | 10 | 20 | 50;
export const HOME_ROWS_PER_TILE_OPTIONS: readonly HomeRowsPerTile[] = [
  5, 10, 20, 50,
];
export function isHomeRowsPerTile(value: unknown): value is HomeRowsPerTile {
  return (HOME_ROWS_PER_TILE_OPTIONS as readonly unknown[]).includes(value);
}

/** The two picker harnesses (#1342 plan Story 2/3). Unknown/legacy provider
 * ids fall back to the incumbent `claude_code`. */
export type PickerHarness = "claude_code" | "codex";

/** One remembered picker choice for a harness (#1342 plan Story 3). */
export interface HarnessChoice {
  model: string;
  mode: string;
  effort: string;
}

export interface UserSettings {
  themeName: ThemeName;
  themeMode: "dark" | "light";
  /**
   * Per-harness remembered picker choices (#1342 plan Story 3; replaced the
   * Claude-only `defaultModel`). The default *harness* itself stays the
   * server-side `default_agent_provider` — this map holds only model/mode/
   * effort per harness, client-local.
   */
  defaultAgentProviderChoices: Record<PickerHarness, HarnessChoice>;
  /** Left-sidebar display mode (settings-page D12, left-sidebar D38). */
  sidebarDisplayMode: SidebarDisplayMode;
  /**
   * Composer subscription-usage indicator window (agent_interaction_ux D8).
   * Default `7d`; `off` hides the read-only usage chip everywhere. Unknown
   * stored values fall back to `7d`. Applies to Claude and Codex subscriptions.
   */
  subscriptionUsageWindow: SubscriptionUsageWindowSetting;
  /**
   * Phase bar style of the workbench header (ui
   * g5.workbench.header-styles). Default `minimal`; unknown stored values
   * fall back to it.
   */
  phaseBarStyle: PhaseBarStyle;
  /**
   * Action bar style of the workbench header (ui
   * g5.workbench.header-styles). Default `dynamic`; unknown stored values
   * fall back to it.
   */
  actionBarStyle: ActionBarStyle;
  /**
   * Whether the center panel's tab strip (Chat, Activity, open documents and
   * files) is rendered (workspaces g5.workspace.ui.center-tabs). Default
   * `false`: the strip stays hidden until the person turns it on. Non-boolean
   * stored values fall back to the default.
   */
  showTabBar: boolean;
  /**
   * Whether the Guided section (the guided experience) renders in the left
   * sidebar under the alignment layout (ui g5.ui.guided-toggle). Default
   * `false`; non-boolean stored values fall back to it. Never widens the
   * instance `guided_journey` feature.
   */
  showGuidedJourney: boolean;
  /**
   * Rows each organization Home tile shows per page (workbench
   * g5.workbench.home-workspaces). Default 5; other stored values fall back
   * to it.
   */
  homeRowsPerTile: HomeRowsPerTile;
  /**
   * Faces of the chat bubble stack above the Chat corner button (ui
   * g5.workspace.ui.chat-bubble-stack). Default `3d`; unknown stored
   * values fall back to it.
   */
  chatBubbleStyle: ChatBubbleStyle;
  /**
   * Bubbles the chat bubble stack shows before the rest fold into a count
   * (ui g5.workspace.ui.chat-bubble-stack). Default 5; stored values outside
   * 1–50 fall back to it.
   */
  chatBubbleLimit: number;
  budgetWarnAt: number;
  editor: EditorSettings;
}

/** Editor preferences — font, indentation, display features */
export interface EditorSettings {
  editorFontSize: number; // 10–24, default 14
  editorTabSize: 2 | 4 | 8; // default 4
  rainbowBrackets: boolean; // default true
  codeFolding: boolean; // default true
  showMinimap: boolean; // default false
  wordWrap: "off" | "on" | "wordWrapColumn" | "bounded"; // default 'off'
  indentGuides: boolean; // default true
  bracketPairGuides: boolean; // default true
  ligatures: boolean; // default false
  stickyScroll: boolean; // default true
  renderWhitespace: "none" | "boundary" | "selection" | "trailing" | "all"; // default 'selection'
}

/** Spend summary from GET /api/v1/workspaces/:id/settings/spend */
export interface SpendBreakdownData {
  periodStart: string;
  periodEnd: string;
  totalSpendUsd: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  limitSpendUsd: number | null;
  limitInputTokens: number | null;
  limitOutputTokens: number | null;
  isSpendLimitReached: boolean;
  unattributedSpendUsd: number;
  otherTaskSpendUsd: number;
  byTask: Array<{
    taskId: string;
    taskType: string;
    taskName: string;
    spendUsd: number;
    inputTokens: number;
    outputTokens: number;
  }>;
}

// ---------------------------------------------------------------------------
// 12. Worker Types
// ---------------------------------------------------------------------------

/** Worker — local or cloud worker instance (mock/display model) */
export interface Worker {
  id: string;
  name: string;
  displayName?: string | null;
  status: "running" | "idle" | "provisioning" | "offline";
  taskId: string | null;
  cpu: number; // 0–100
  mem: number; // 0–100
  uptime: string;
  taskName: string;
  agentProviders?: string[];
}

export type ChatAgentProvider = "claude_code" | "codex";

/**
 * Per-turn Codex chat settings carried under
 * `ChatSessionDTO.providerState.codex.settings`, mirroring the
 * `providerState.claudeCode.settings` namespace. All values are Codex-NATIVE and
 * passed through verbatim (no translation onto Claude Code modes):
 *   - `model` — a Codex catalog model id (lib/codexModels.ts)
 *   - `permissionMode` — compound "<approval_policy>/<sandbox_mode>", e.g.
 *     "never/workspace-write"
 *   - `effort` — native model_reasoning_effort level, e.g. "xhigh"
 * `providerState` stays typed as the opaque `Record<string, unknown> | null` on
 * `ChatSessionDTO`; this shape is read defensively by the chat composer.
 */
export interface CodexProviderSettings {
  model?: string;
  permissionMode?: string;
  effort?: string;
}

// ---------------------------------------------------------------------------
// 13. File & Doc Types
// ---------------------------------------------------------------------------

/** File tree entry from GET /api/v1/branches/:branchId/files */
export interface FileTreeEntry {
  name: string;
  path: string; // relative to repo root
  type: "file" | "directory";
  size: number | null; // bytes, null for directories
  children?: FileTreeEntry[]; // present when depth > 1
}

/** Doc list item from GET /api/v1/workspaces/:id/docs */
export interface DocNode {
  id: string;
  name: string;
  docType: string; // 'architecture' | 'plan' | 'guide' | 'spec' etc.
  filePath: string | null;
  documentSource?: "user" | "auxiliary" | null;
  linkedNodeCount: number;
  source?: "indexed" | "overlay" | "merged" | string;
  scope?: string;
  workspaceId?: string | null;
  repoId?: string | null;
  draftDirty?: boolean;
  isVirtual?: boolean;
  lastUpliftedAt: string | null;
  lastDownliftedAt: string | null;
  contentUpdatedAt: string | null;
  createdAt: string;
  updatedAt: string;
  /** Whether this is a future (not-yet-implemented) view document */
  isFuture?: boolean;
  /** Optional folder for nesting in sidebar */
  folder?: string;
  /** Whether the doc is pinned to top of sidebar */
  pinned?: boolean;
  /** Nested child docs (for hierarchical rendering) */
  children?: DocNode[];
  /** Display title (populated for sidebar rendering) */
  title?: string;
  /** Category tag for icon/status derivation */
  category?: string;
  /** Short preview snippet */
  preview?: string;
  /** Full content body */
  content?: string;
  contentHash?: string | null;
  repoOid?: string | null;
  snapshotAt?: string | null;
  vmId?: string | null;
}

/** File tree node (V1 mock model — retained for compatibility) */
export interface FileTreeNode {
  name: string;
  type: "file" | "dir";
  children?: FileTreeNode[];
}

/** Project document (V1 mock model — retained for compatibility) */
export interface ProjectDoc {
  id: string;
  title: string;
  category: "architecture" | "plan" | "guide";
  content: string;
  preview: string;
  lastUpdated: string;
}

// ---------------------------------------------------------------------------
// 14. Diff Types
// ---------------------------------------------------------------------------

/** Diff line — line-level diff data */
export interface DiffLine {
  type: "context" | "addition" | "deletion";
  content: string;
  oldLine?: number;
  newLine?: number;
}

/** Diff hunk — group of contiguous diff lines */
export interface DiffHunk {
  startLine: number;
  header: string; // e.g., '@@ -5,8 +5,11 @@ ## Overview'
  lines: DiffLine[];
}

/** Diff data — complete diff for a file/node */
export interface DiffData {
  /**
   * Repo the diff was computed in. A task diff spans every repo the task branch
   * touches, so a file path is only unique within a repo — `repoId` keeps the
   * same relative path in two repos unambiguous.
   */
  repoId: string;
  repoName: string;
  file: string;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

export type ChangeGraphColumn = "intents" | "ontology" | "source_code";
export type ChangeGraphState = "added" | "modified" | "deleted" | "unchanged";
export type SourceControlChangeGraphComparison = "workspace" | "uncommitted";
export type SourceControlChangeGraphOrigin =
  | "committed"
  | "uncommitted"
  | "both";

/** Why a repository shows committed content only (g5.diff-graph.modes). */
export type ChangeGraphCommittedReason =
  | "not_owner"
  | "vm_not_running"
  | "unpublished_commits"
  | "status_unavailable"
  | "report_unavailable"
  | "workspace_synchronizing";

export interface SourceControlChangeGraphLiveContent {
  repoId: string;
  repoName: string;
  includesUncommitted: boolean;
  reason?: ChangeGraphCommittedReason | string;
  ontologyLive: boolean;
}

export interface SourceControlChangeGraphRange {
  baseKind: "merge_base" | "accepted_head";
  parentWorkspaceId?: string;
  parentWorkspaceName?: string;
  repositoryCount: number;
  liveContent?: SourceControlChangeGraphLiveContent[];
}

export interface SourceControlChangeGraphRoute {
  repoId: string;
  workspaceId: string;
  filePath?: string;
  fileType: "chat" | "external_doc" | "view" | "ontology" | "code";
  recordId?: string;
  sessionId?: string;
}

export interface SourceControlChangeGraphChangedRecord {
  id: string;
  label: string;
  changeState: ChangeGraphState;
  /** Statement Type (I, D or Q) of the version shown for the change. */
  kind?: "I" | "D" | "Q" | string;
  content?: string;
  isHuman: boolean;
}

/** An added or edited link whose target exists on neither side. */
export interface SourceControlChangeGraphMissingTarget {
  statementId: string;
  href: string;
  text: string;
}

export interface SourceControlChangeGraphNode {
  id: string;
  column: ChangeGraphColumn;
  intentGroup?: "conversations" | "documents" | "other";
  /** Folder below `.g5/ontology`; absent for top-level pages. */
  ontologyGroup?: string;
  title: string;
  artifactType: string;
  path?: string;
  changeState: ChangeGraphState;
  changeOrigin?: SourceControlChangeGraphOrigin;
  connectionState: "connected" | "bridge" | "unlinked";
  additions?: number;
  deletions?: number;
  changedRecords: SourceControlChangeGraphChangedRecord[];
  /** The statement rule refused this file version; no statement is classified. */
  comparisonWarning?: string;
  missingTargets?: SourceControlChangeGraphMissingTarget[];
  route: SourceControlChangeGraphRoute;
  hasHumanBlocks: boolean;
}

/** Authored link kinds (g5.diff-graph.relationships). Citations, not dependencies. */
export type ChangeGraphRelationship =
  | "expands"
  | "reference"
  | "provenance"
  | "implementation"
  | "project_document";

export interface SourceControlChangeGraphEvidence {
  id: string;
  sourceFileType: string;
  sourceFile: string;
  sourceId: string;
  targetFileType: string;
  targetFile: string;
  targetId: string;
  relationship: ChangeGraphRelationship | string;
  relevance?: number;
  changeState: ChangeGraphState;
  /** The target statement or file was deleted in this comparison. */
  targetDeleted?: boolean;
}

export interface SourceControlChangeGraphEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  evidence: SourceControlChangeGraphEvidence[];
}

export interface SourceControlChangeGraphResponse {
  revision: string;
  comparison?: SourceControlChangeGraphComparison;
  range?: SourceControlChangeGraphRange;
  nodes: SourceControlChangeGraphNode[];
  edges: SourceControlChangeGraphEdge[];
  diagnostics: Array<{ code: string; count: number }>;
  truncated: boolean;
}

export type SourceControlChangeGraphDiffAvailability =
  | "available"
  | "binary"
  | "oversized"
  | "unavailable";

export type SourceControlChangeGraphDiffReason =
  | "binary_content"
  | "input_limit_exceeded"
  | "retained_evidence_missing"
  | "output_limit_reached";

export interface SourceControlChangeGraphDiffEvidence {
  availability: SourceControlChangeGraphDiffAvailability;
  reason?: SourceControlChangeGraphDiffReason;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
  returnedLineCount: number;
  omittedLineCount: number;
  truncated: boolean;
  partialFinalHunk: boolean;
}

export interface SourceControlChangeGraphSemanticRecordDiff {
  id: string;
  recordKind: "I" | "D" | "Q" | string;
  title: string;
  changeState: ChangeGraphState;
  diff: SourceControlChangeGraphDiffEvidence;
  isHuman: boolean;
  author?: string;
  authorSource?: string;
  timestamp?: string;
  timestampSource?: string;
}

export type SourceControlChangeGraphNodeDetails =
  | {
      revision: string;
      comparison?: SourceControlChangeGraphComparison;
      nodeId: string;
      kind: "file_diff";
      diff: SourceControlChangeGraphDiffEvidence;
    }
  | {
      revision: string;
      comparison?: SourceControlChangeGraphComparison;
      nodeId: string;
      kind: "semantic_diff";
      records: SourceControlChangeGraphSemanticRecordDiff[];
      otherFileChanges?: SourceControlChangeGraphDiffEvidence;
      truncated: boolean;
    }
  | {
      revision: string;
      comparison?: SourceControlChangeGraphComparison;
      nodeId: string;
      kind: "unchanged_context";
    };

export type RebaseChangeGraphConflictKind = "same_node" | "same_block";

export interface RebaseChangeGraphConflict {
  id: string;
  artifactKey: string;
  sourceNodeId: string;
  targetNodeId: string;
  kind: RebaseChangeGraphConflictKind;
  overlappingBlockIds: string[];
  overlapBlockCount?: number;
}

export interface RebaseChangeGraphWorkspace {
  id: string;
  name: string;
}

export interface RebaseChangeGraphResponse {
  revision: string;
  sourceWorkspace: RebaseChangeGraphWorkspace;
  targetWorkspace: RebaseChangeGraphWorkspace;
  commonBaseRepositoryCount: number;
  source: SourceControlChangeGraphResponse;
  target: SourceControlChangeGraphResponse;
  conflicts: RebaseChangeGraphConflict[];
}

export type RebaseChangeGraphNodeDetail =
  | {
      kind: "file_diff";
      diff: SourceControlChangeGraphDiffEvidence;
    }
  | {
      kind: "semantic_diff";
      records: SourceControlChangeGraphSemanticRecordDiff[];
      otherFileChanges?: SourceControlChangeGraphDiffEvidence;
      truncated: boolean;
    }
  | { kind: "unchanged_context" };

export interface RebaseChangeGraphSideDetails {
  node: SourceControlChangeGraphNode;
  changedLineCount: number;
  changedBlockCount: number;
  detail: RebaseChangeGraphNodeDetail;
}

export interface RebaseChangeGraphNodeDetailsResponse {
  revision: string;
  artifactKey: string;
  title: string;
  artifact: string;
  path?: string;
  source?: RebaseChangeGraphSideDetails;
  target?: RebaseChangeGraphSideDetails;
  overlapLineCount: number;
  overlapBlockIds: string[];
  overlapBlockCount: number;
}

/** Diff decoration for Monaco editor overlay */
export interface DiffDecoration {
  startLine: number;
  endLine: number;
  type: "add" | "remove" | "modify";
}

// ---------------------------------------------------------------------------
// 16. Notification Types
// ---------------------------------------------------------------------------

/** Notification item (V1 mock model) */
export interface NotificationItem {
  id: string;
  type: "task_complete" | "ai_input" | "mention" | "comment";
  title: string;
  body: string;
  time: string;
  read: boolean;
  taskId?: string;
}

/** G5 notification from WebSocket notification events */
export interface G5Notification {
  id: string;
  /**
   * The workspace whose inbox stores this row (read state is persisted
   * there); null for client-only toasts that have no server row.
   */
  workspaceId: string | null;
  severity: "info" | "warning" | "error";
  category:
    | "task_blocked"
    | "build_failure"
    | "test_failure"
    | "spend_warning"
    | "spend_limit"
    | "agent_error"
    | "worker_terminated"
    | "merge_conflict"
    | "mention"
    | "comment"
    | "review"
    | "pull_request";
  title: string;
  message: string;
  taskId: string | null;
  commentId: string | null;
  replyId: string | null;
  actions: NotificationAction[];
  createdAt: string;
  expiresAt: string | null;
  readAt: string | null;
}

/** Action button on a notification */
export interface NotificationAction {
  label: string;
  action: "navigate" | "open_chat";
  target: string;
}

// ---------------------------------------------------------------------------
// 17. Editor Types
// ---------------------------------------------------------------------------

/** Editor tab — tracks open files in center panel */
export interface EditorTab {
  id: string;
  nodeId: string;
  type: "spec" | "code" | "document" | "source";
}

export type GuideDocId =
  | "getting-started"
  | "onboarding-build-from-scratch"
  | "onboarding-pm-poc-legacy";

/** Tab item — discriminated union for tab types */
export type TabItem =
  | {
      type: "doc";
      liveAuthorityKey?: string;
      docId: string;
      name: string;
    }
  | { type: "guide_doc"; guideId: GuideDocId; name: string }
  | { type: "intent_log"; name: string }
  | { type: "pull_requests"; name: string }
  | {
      type: "file";
      liveAuthorityKey?: string;
      filePath: string;
      name: string;
      branchId?: string;
      /** Optional 1-based [start, end] line range to reveal/highlight on open
       *  (citation navigation, chat_service D54 / message_rendering D7). */
      lineRange?: [number, number];
    }
  | { type: "workspace_changes"; name: string }
  | { type: "source_control_diff"; name: string; selectedFileKey?: string }
  | { type: "source_control_change_graph"; name: string }
  | {
      type: "ontology_changes";
      name: string;
      /** Scroll to one file, e.g. a deleted file opened from Diff Graph. */
      focusFile?: { repoId: string; path: string; seq: number };
    }
  | {
      type: "source_control_commit_diff";
      name: string;
      repoId: string;
      repoName: string;
      branchId: string;
      sha: string;
    }
  | { type: "workspace_settings"; name: string }
  | { type: "workspace_terminal"; name: string }
  | { type: "worker_hosts"; name: string }
  | { type: "org_members"; name: string }
  | { type: "org_approvals"; name: string }
  | { type: "org_github_app"; name: string }
  | { type: "org_admin"; name: string }
  | { type: "workspace_approvals"; name: string }
  | { type: "workspace_review"; name: string }
  | { type: "workspace_details"; name: string }
  | { type: "workspace_activity"; name: string }
  | {
      type: "external_issue";
      repositoryId: string;
      number: number;
      name: string;
    };

/** Connected collaborator — cursor position for live workspace file awareness */
export interface Collaborator {
  id: string;
  name: string;
  avatar: string;
  color: string;
  cursor: { line: number; col: number };
}

/** Comment thread — shared across editors (V1 mock model) */
export interface CommentThread {
  id: string;
  lineStart: number;
  lineEnd: number;
  resolved: boolean;
  comments: {
    author: string;
    avatar: string;
    text: string;
    time: string;
  }[];
}

// ---------------------------------------------------------------------------
// 18. Instance & Spend Types
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GitHub App organization binding (routes/github_app.rs)
// ---------------------------------------------------------------------------

/** One GitHub repository covered by a binding snapshot. */
export interface GithubAppCoveredRepositoryDTO {
  githubRepoId: number;
  fullName: string;
}

/** The latest recorded binding snapshot; not a live GitHub verification. */
export interface GithubAppBindingDTO {
  version: number;
  disposition: "active" | "disconnected";
  provenance:
    | "installation_callback"
    | "coverage_change"
    | "disconnect"
    | "reinstall"
    | "installation_refresh";
  installationId: number;
  accountLogin: string;
  /** GitHub-granted installation permissions, e.g. `{ issues: "write" }`. */
  permissions: Record<string, string>;
  recordedAt: string;
  coveredRepositories: GithubAppCoveredRepositoryDTO[];
}

/** GET /api/v1/orgs/:orgId/github-app/binding */
export interface GithubAppBindingStatusResponseDTO {
  notice: string;
  binding: GithubAppBindingDTO | null;
}

/** POST connection-attempt / reconnect-attempt: a server-built GitHub URL. */
export interface GithubAppRedirectDTO {
  redirectUrl: string;
}

/** One repository the active installation can access, live from GitHub. */
export interface GithubAppInstallationRepositoryDTO {
  githubRepoId: number;
  fullName: string;
  private: boolean;
  covered: boolean;
}

/** GET /api/v1/orgs/:orgId/github-app/binding/repositories */
export interface GithubAppInstallationRepositoriesDTO {
  bindingVersion: number;
  repositories: GithubAppInstallationRepositoryDTO[];
}

/** PUT /api/v1/orgs/:orgId/github-app/binding/coverage */
export interface UpdateGithubAppCoverageInput {
  bindingVersion: number;
  githubRepoIds: number[];
}

/** POST /api/v1/orgs/:orgId/github-app/binding/disconnect */
export interface DisconnectGithubAppBindingInput {
  bindingVersion: number;
}

/** POST /api/v1/orgs/:orgId/github-app/binding/refresh */
export interface RefreshGithubAppBindingInput {
  bindingVersion: number;
}

/** GET /api/v1/orgs/:orgId/github-app/binding/management-url */
export interface GithubAppManagementUrlDTO {
  managementUrl: string;
}

// ---------------------------------------------------------------------------
// Issues — issue domains (GitHub repositories and the native G5 tracker),
// search, workspace links and reads
// ---------------------------------------------------------------------------
// Typed failures carry `error.details.issueError` (see ExternalIssueErrorCode);
// a failure is never an empty result.

/** The tracker an issue lives in: GitHub, or G5's own (g5.issue.native). */
export type IssueProvider = "github" | "g5";

/** `error.details.issueError` / per-domain and per-link failure codes. */
export type ExternalIssueErrorCode =
  | "unavailable"
  | "rate_limited"
  | "access_denied"
  | "not_found"
  | "outside_domain"
  | "not_an_issue"
  | "binding_needs_attention"
  | "invalid_reference"
  | "invalid_query";

/** Why a configured issue domain cannot currently be read. */
export type IssueDomainUnavailableReason =
  | "no_binding"
  | "binding_inactive"
  | "repository_uncovered"
  | "issues_permission_missing";

/**
 * One organization issue domain: a GitHub repository, or the organization's
 * native G5 tracker (at most one).
 */
export interface IssueDomainDTO {
  id: string;
  trackerId: string;
  provider: IssueProvider;
  containerKind: "github_repository" | "g5_project";
  /** GitHub numeric repository id, decimal string; `1` for the native tracker. */
  externalContainerId: string;
  /** `owner/name`, or the native tracker's issue key (`ACME`). */
  displayKey: string;
  isDefaultCreateTarget: boolean;
  /** When false, issue bodies/comments render as restricted Markdown. */
  contentTrusted: boolean;
  /**
   * An org admin allows G5 comments on this domain's issues and, for the
   * default creation target, issue creation from G5. GitHub only; the
   * native tracker ignores it.
   */
  commentsEnabled: boolean;
  /** Recorded-state check against the latest GitHub App binding. */
  available: boolean;
  unavailableReason: IssueDomainUnavailableReason | null;
  createdAt: string;
}

/** GET /api/v1/orgs/:orgId/issue-domains */
export interface IssueDomainListDTO {
  domains: IssueDomainDTO[];
}

/** POST /api/v1/orgs/:orgId/issue-domains */
export type CreateIssueDomainInput =
  | { githubRepoId: string; contentTrusted: boolean }
  /** The native tracker; `key` is its issue key (`ACME` → `ACME-12`). */
  | { provider: "g5"; key: string; contentTrusted: boolean };

/** PATCH /api/v1/orgs/:orgId/issue-domains/:domainId */
export interface UpdateIssueDomainInput {
  domainId: string;
  contentTrusted?: boolean;
  /** Allow G5 comments (and issue creation) for this domain. */
  commentsEnabled?: boolean;
}

export interface ExternalIssueLocatorDTO {
  repositoryId: string;
  number: number;
}

export interface ExternalIssueUserDTO {
  /** GitHub login; a native issue's member is named by their email. */
  login: string;
  avatarUrl: string | null;
  /** The G5 user, on native issues. */
  userId?: string | null;
  /** The G5 user's name, on native issues; shown in place of `@login`. */
  displayName?: string | null;
}

/**
 * A label as issues show it, or a catalog label (g5.issue.labels.contract).
 * Labels are named by name, matched without regard to case; `id` is opaque
 * and unique in its domain only.
 */
export interface ExternalIssueLabelDTO {
  /** The catalog label's id; absent where an issue read does not carry it. */
  id?: string;
  name: string;
  /** Six lowercase hex digits without `#`, or `null`. */
  color: string | null;
  description?: string;
}

/** GET /api/v1/orgs/:orgId/issue-domains/:domainId/labels?page=N */
export interface LabelPageDTO {
  domainId: string;
  displayKey: string;
  /** At most 100, ordered by name. */
  labels: ExternalIssueLabelDTO[];
  /** The next page's number while more labels exist. */
  nextPage: number | null;
}

/**
 * A catalog create, edit or delete. `unknown`: GitHub gave no definite
 * answer; reload the catalog before retrying.
 */
export interface LabelWriteDTO {
  outcome: "applied" | "unknown";
  label: ExternalIssueLabelDTO | null;
  /** `null` for the native tracker. */
  githubLogin: string | null;
  message: string | null;
}

/** POST …/issue-domains/:domainId/labels */
export interface CreateLabelInput {
  /** The domain the editor was opened for, bound at submission. */
  domainId: string;
  name: string;
  /** Six hex digits, with or without `#`; omitted for the tracker's default. */
  color?: string;
  description?: string;
}

/** PATCH …/issue-domains/:domainId/labels/:labelId — only the fields given. */
export interface EditLabelInput {
  domainId: string;
  labelId: string;
  name?: string;
  color?: string;
  /** Empty clears it. */
  description?: string;
}

/** DELETE …/issue-domains/:domainId/labels/:labelId */
export interface DeleteLabelInput {
  domainId: string;
  labelId: string;
}

/**
 * POST …/issues/{g5/:number | github/:repositoryId/:number}/labels — label
 * names to add and to remove, never a replacement set.
 */
export interface IssueLabelChangeInput {
  /** The issue the editor was opened for, bound at submission. */
  locator: ExternalIssueLocatorDTO;
  addLabels: string[];
  removeLabels: string[];
}

/** One requested label change and what became of it. */
export interface LabelChangeOutcomeDTO {
  label: string;
  change: "add" | "remove";
  outcome: "applied" | "refused" | "unknown" | "not_attempted";
  message?: string;
}

export interface IssueLabelChangeDTO {
  /**
   * `applied` (every change), `partial` (a refusal stopped the rest) or
   * `unknown` (no definite answer; re-read before retrying).
   */
  outcome: "applied" | "partial" | "unknown";
  changes: LabelChangeOutcomeDTO[];
  /** The issue as re-read, with its labels; `null` when that read failed. */
  issue: ExternalIssueSummaryDTO | null;
  /** `null` for a native issue. */
  githubLogin: string | null;
  message: string | null;
}

/** The workspace an issue is primary for; hidden when the viewer lacks access. */
export type ExternalIssuePrimaryWorkspaceDTO =
  | { visible: true; workspaceId: string; name: string }
  | { visible: false };

export interface ExternalIssueSummaryDTO {
  provider: IssueProvider;
  trackerId: string;
  domainId: string;
  /** GitHub REST issue id, or the native issue's G5 id; decimal string. */
  externalIssueId: string;
  /** Native issues: `{ repositoryId: "g5", number }`. */
  locator: ExternalIssueLocatorDTO;
  /** `owner/name#12`, or `ACME-12` for a native issue. */
  displayKey: string;
  /** GitHub `html_url`, or the native issue's G5 page URL. */
  canonicalUrl: string;
  title: string;
  state: "open" | "closed";
  stateReason: string | null;
  assignees: ExternalIssueUserDTO[];
  labels: ExternalIssueLabelDTO[];
  /** Native issues: the edit version an edit must match. */
  updatedAt: string;
  /** Copied from the issue's domain. */
  contentTrusted: boolean;
  /** `null` when no workspace holds the primary link. */
  primaryWorkspace: ExternalIssuePrimaryWorkspaceDTO | null;
}

export interface ExternalIssueCommentDTO {
  id: string;
  author: ExternalIssueUserDTO | null;
  body: string;
  createdAt: string;
  updatedAt: string;
  url: string;
}

export interface ExternalIssueWorkspaceLinkRefDTO {
  /** Absent, like the name, when the viewer cannot access the workspace. */
  workspaceId?: string;
  /** `null` when the viewer cannot access the workspace. */
  workspaceName: string | null;
  role: "primary" | "related";
}

/**
 * GET /api/v1/orgs/:orgId/issues/github/:repositoryId/:number, or
 * GET /api/v1/orgs/:orgId/issues/g5/:number for a native issue.
 */
export interface ExternalIssueDetailDTO extends ExternalIssueSummaryDTO {
  body: string | null;
  author: ExternalIssueUserDTO | null;
  createdAt: string;
  closedAt: string | null;
  commentCount: number;
  /** Oldest first; at most the first 100. */
  comments: ExternalIssueCommentDTO[];
  /** True when `commentCount > comments.length`. */
  commentsTruncated: boolean;
  links: ExternalIssueWorkspaceLinkRefDTO[];
}

export interface ExternalIssueDomainErrorDTO {
  domainId: string;
  displayKey: string;
  issueError: ExternalIssueErrorCode | string;
  message: string;
}

export type ExternalIssueStateFilter = "open" | "closed" | "all";

/** Issue search by assignee: anyone, the caller's connected GitHub account, or no one. */
export type ExternalIssueAssigneeFilter = "any" | "me" | "none";

/** GET /api/v1/orgs/:orgId/issues/search */
export interface ExternalIssueSearchResponseDTO {
  results: ExternalIssueSummaryDTO[];
  domainErrors: ExternalIssueDomainErrorDTO[];
  /** 0 means no issue domain is configured. */
  domainCount: number;
  /** More matches existed across domains than fit on this page. */
  truncated: boolean;
}

export type WorkspaceIssueLinkRole = "primary" | "related";

// ---------------------------------------------------------------------------
// Workspace Activity (GET /api/v1/workspaces/:id/activity)
// ---------------------------------------------------------------------------

interface ActivityEntryBase {
  /** Stable identity, e.g. `chat:<uuid>` or `github-comment:<id>`. */
  key: string;
  at: string;
  /**
   * `github` / `g5`: the shared issue discussion (a native issue's is
   * `g5`); `workspace`: G5 context.
   */
  source: "github" | "g5" | "workspace";
}

/** A preview run's live stage (ui g5.ui.activity.preview-row). */
export type PreviewRunStage =
  | "queued"
  | "starting"
  | "building"
  | "serving"
  | "closing";

/**
 * Why a preview run ended, structured so the client words it once for
 * Activity and the run log (ui g5.ui.activity.preview-row).
 */
export interface PreviewEndingDTO {
  /** ended (exited on its own), closed (owner Close), stopped, failed, did_not_start. */
  kind: "ended" | "closed" | "stopped" | "failed" | "did_not_start";
  /**
   * For `failed`: app_exited, module, timed_out, limit_exceeded, interrupted
   * or vm_lost. For `stopped` without a retained reason: unstarted when the
   * module never ran.
   */
  cause?:
    | "app_exited"
    | "module"
    | "timed_out"
    | "limit_exceeded"
    | "interrupted"
    | "vm_lost"
    | "unstarted";
  /** The retained stop reason (g5.preview.service.stop-reason), when recorded. */
  reason?: string;
  /** How a close ended: clean, failed, killed, or lost (VM gone before cleanup finished). */
  close?: string;
  exitCode?: number | null;
  signal?: number | null;
  /** The module's or dispatcher's own words, when it gave any. */
  message?: string;
}

/**
 * A request to open the preview dialog for a run (ui
 * g5.ui.activity.preview-row): with its run log when the run has one.
 */
export interface PreviewOpenRequest {
  taskId: string;
  log: boolean;
}

/** A preview run as its Activity row carries it. */
export interface PreviewRunActivityDTO {
  profile: string | null;
  port: number | null;
  /** Present while the task is live. */
  stage?: PreviewRunStage;
  /** Present once the task ended. */
  ending?: PreviewEndingDTO;
}

export type ActivityEntryDTO = ActivityEntryBase &
  (
    | {
        kind: "workspace_comment";
        comment: ReviewComment;
      }
    | {
        kind: "workspace_reply";
        commentId: string;
        content: string;
        actorName: string | null;
        phase: WorkspacePhase | null;
      }
    | {
        kind: "approval_requested";
        note?: string | null;
        cancelled?: boolean;
        requestId: string;
        definitionName: string;
        phase: WorkspacePhase | null;
        actorName: string | null;
        reviewerName: string | null;
      }
    | { kind: "workspace_created"; actorName: string | null }
    | {
        /** The issue's description, which opens its discussion. */
        kind: "issue_opened";
        author: ExternalIssueUserDTO | null;
        title: string;
        body: string | null;
        url: string;
      }
    | {
        kind: "issue_comment";
        /** GitHub's comment id, for editing one's own comment. */
        commentId: string;
        author: ExternalIssueUserDTO | null;
        body: string;
        url: string;
        /** When GitHub last changed the comment; an edit must match it. */
        updatedAt: string;
      }
    | {
        kind: "issue_event";
        actor: ExternalIssueUserDTO | null;
        event: string;
        detail: string | null;
        stateReason: string | null;
      }
    | {
        kind: "chat";
        sessionId: string;
        title: string | null;
        messageCount: number;
        lastActivityAt: string;
        /**
         * The agent task's status while the conversation is ongoing
         * (`queued`, `running` or `waiting`); only on Current entries.
         */
        taskStatus?: string;
      }
    | {
        kind: "task";
        taskId: string;
        name: string;
        taskKind: string;
        status: string;
        /** Why the task ended (e.g. a preview's `idle_stop`), when known. */
        reason: string | null;
        /**
         * A preview run's profile, port, stage while live and structured
         * ending once ended (ui g5.ui.activity.preview-row); absent for
         * other task kinds.
         */
        preview?: PreviewRunActivityDTO;
      }
    | {
        /** A commit on a workspace branch since its base (G5's git). */
        kind: "commit";
        repoId: string;
        repoName: string;
        branchName: string;
        sha: string;
        shortSha: string;
        summary: string;
        authorName: string;
      }
    | {
        kind: "approval_decision";
        /** Null identifies retained decisions without a PR association. */
        pullRequestId?: string | null;
        reviewedGitHeads?: ReviewHead[];
        thread?: ReviewComment | null;
        definitionName: string | null;
        outcome: string;
        /** Phase the decision settled (`be` | `se` | `ie`). */
        phase: string;
        actorType: string;
        actorName: string | null;
        /** Human comment; always null for agent decisions (never a report clip). */
        comment: string | null;
        /** Agent-authored summary (report schema 5+); null otherwise. */
        summary?: string | null;
        /** True only for legacy decisions recorded without a request. */
        historical?: boolean;
      }
    | {
        /**
         * An owner moved the workspace's phase: one durable
         * `workspace_phase_events` row (workbench g5.workbench.phases.state).
         */
        kind: "phase_changed";
        fromPhase: WorkspacePhase;
        toPhase: WorkspacePhase;
        actor: ActivityActorDTO | null;
      }
    | {
        /**
         * An owner marked a guided-journey step done or not done: one
         * durable `workspace_journey_events` row (workbench
         * g5.workbench.journey.marks).
         */
        kind: "journey_step";
        stepId: string;
        done: boolean;
        /** The owner skipped the step; absent on older servers. */
        skipped?: boolean;
        actor: ActivityActorDTO | null;
      }
    | {
        /** A lifecycle event of a pull request this workspace is the source of. */
        kind: "pull_request";
        pullRequestId: string;
        /**
         * `created | source_rebased | merge_blocked | merged | closed |
         * provider_created | provider_merged | provider_action_required |
         * provider_failed | auto_merge_enabled | auto_merge_disabled |
         * merge_overridden | provider_merged_outside`
         * (sync-only events are not projected). `merge_overridden`: actor is
         * the overriding person, message the reason; `provider_merged_outside`:
         * message is the repository's full name.
         */
        eventType: string;
        title: string;
        targetWorkspaceName: string | null;
        sourceWorkspaceName?: string | null;
        /**
         * The pull request merges into this workspace: the target's timeline
         * shows how incoming work landed (`merged`, `merge_overridden`).
         */
        incoming?: boolean;
        /** The event's own message (why a merge was blocked, a provider error). */
        message: string | null;
        /** Repositories the pull request publishes to (0 while local only). */
        repositoryCount: number;
        /**
         * Its provider PR, only when it spans exactly one repository (provider
         * events name the aggregate PR, not the repository); else null.
         */
        providerRepoFullName: string | null;
        providerPrNumber: number | null;
        providerPrUrl: string | null;
        actor: ActivityActorDTO | null;
        /**
         * `provider_failed` only, else null: the failures since the PR's last
         * completed publication. Only the newest unresolved one is projected,
         * so this entry stands for all of them.
         */
        failureCount: number | null;
        /** When the first of those failures happened; else null. */
        firstFailedAt: string | null;
      }
  );

/** The G5 user behind a workspace record, when it still resolves. */
export interface ActivityActorDTO {
  userId: string;
  displayName: string;
}

export interface WorkspaceActivityDTO {
  primaryIssue: {
    /** Absent on older servers, which serve GitHub issues only. */
    provider?: IssueProvider;
    linkId: string;
    displayKey: string;
    canonicalUrl: string;
    locator: ExternalIssueLocatorDTO;
    contentTrusted: boolean;
  } | null;
  /** Why the issue discussion is missing from this page, if it is. */
  issueError: { issueError: string; message: string } | null;
  /** GitHub returned only the first page of the issue's timeline. */
  issueHistoryTruncated: boolean;
  /** Newest first, by last activity. */
  entries: ActivityEntryDTO[];
  /** Pass as `before` to load older entries; `null` at the beginning. */
  earlierCursor: string | null;
  /** Whether the viewer can comment from G5; `null` without a primary issue. */
  composer: CommentComposerDTO | null;
}

/**
 * Ongoing work (GET /api/v1/workspaces/:id/activity/current, ui
 * g5.ui.activity-current): conversations with a queued, running or waiting
 * agent task and preview/command tasks in those states, newest first.
 */
export interface WorkspaceActivityCurrentDTO {
  entries: ActivityEntryDTO[];
}

export type CommentComposerState =
  | "ready"
  | "connection_required"
  | "comments_disabled"
  | "public_destination"
  | "visibility_unknown"
  | "unavailable";

/** The destination a comment is addressed to; submitted back unchanged. */
export interface CommentDestinationDTO {
  trackerId: string;
  externalIssueId: string;
}

export interface CommentComposerDTO {
  /** Absent on older servers, which serve GitHub issues only. */
  provider?: IssueProvider;
  state: CommentComposerState;
  /** The GitHub account comments are posted as, once connected; `null` on native issues. */
  githubLogin: string | null;
  destination: CommentDestinationDTO;
  displayKey: string;
}

/**
 * A comment written to GitHub from G5 (`POST`/`PATCH` of an issue comment,
 * from a workspace or the issue page). Nothing is recorded: a refusal is an
 * HTTP error, and `outcome_unknown` means GitHub did not answer clearly;
 * sending again with the same idempotency key is safe, since G5 looks for
 * the comment before posting.
 */
export interface GithubCommentResultDTO {
  id: string;
  provider: "github";
  state: "published" | "outcome_unknown";
  /** `comment` (posted) or `edit` (replaced an existing comment's text). */
  kind: "comment" | "edit";
  /** `agent` when the workspace's agent posted it as the workspace owner. */
  origin: "person" | "agent";
  destination: "issue";
  displayKey: string;
  githubLogin: string;
  /** The comment as written, without the G5 marker. */
  body: string;
  /** The exact text sent to GitHub. */
  payload: string;
  externalCommentUrl: string | null;
  error: null;
  createdAt: string;
  publishedAt: string | null;
  /** Why the outcome is unknown. */
  message: string | null;
}

/**
 * A comment stored on a native G5 issue: published at once (`id`
 * `g5:<commentId>`, `githubLogin` null), with the stored `comment`.
 */
export interface NativeCommentResultDTO {
  id: string;
  provider: "g5";
  state: "published";
  kind: "comment" | "edit";
  origin: "person" | "agent";
  destination: "issue";
  displayKey: string;
  githubLogin: null;
  body: string;
  /** The body, as stored. */
  payload: string;
  externalCommentUrl: string | null;
  error: null;
  createdAt: string;
  publishedAt: string | null;
  comment: ExternalIssueCommentDTO;
}

/** What an issue comment write answers, by tracker. */
export type CommentResultDTO = GithubCommentResultDTO | NativeCommentResultDTO;

export interface PostIssueCommentInput {
  body: string;
  idempotencyKey: string;
  destination: CommentDestinationDTO;
}

export interface EditIssueCommentInput extends PostIssueCommentInput {
  /** The comment's tracker: native edits go to `/activity/comments/g5/:id`. */
  provider?: IssueProvider;
  commentId: string;
  /** The comment's `updatedAt` when the editor opened. */
  expectedUpdatedAt: string;
}

/** The caller's own GitHub connection. */
export interface GithubConnectionStatusDTO {
  connected: boolean;
  githubLogin: string | null;
}

export interface WorkspaceIssueLinkDTO {
  /** Absent on older servers, which serve GitHub issues only. */
  provider?: IssueProvider;
  id: string;
  workspaceId: string;
  trackerId: string;
  externalIssueId: string;
  displayKey: string;
  canonicalUrl: string;
  locator: ExternalIssueLocatorDTO;
  role: WorkspaceIssueLinkRole;
  linkedBy: string;
  linkedAt: string;
  /** Live GitHub summary; `null` when `issueError` explains why. */
  issue: ExternalIssueSummaryDTO | null;
  issueError: {
    issueError: ExternalIssueErrorCode | string;
    message: string;
  } | null;
}

// ---------------------------------------------------------------------------
// Issue creation (g5.external-issues.creation) — the organization's default
// creation target, under the caller's own GitHub account
// ---------------------------------------------------------------------------

export type IssueCreateTargetState =
  | "ready"
  | "no_target"
  | "comments_disabled"
  | "public_destination"
  | "visibility_unknown"
  | "connection_required"
  | "unavailable";

/** Where a new issue goes: the organization's default creation target. */
export interface IssueCreationDestinationDTO {
  /** Absent on older servers, which serve GitHub issues only. */
  provider?: IssueProvider;
  domainId: string;
  trackerId: string;
  /** GitHub numeric repository id, decimal string; `g5` for the native tracker. */
  repositoryId: string;
  /** `owner/name`, or the native tracker's issue key (`ACME`). */
  displayKey: string;
  /** An untrusted domain reviews the exact payload before creating. */
  contentTrusted: boolean;
}

/** GET /api/v1/orgs/:orgId/issues/create-target */
export interface IssueCreateTargetDTO {
  state: IssueCreateTargetState;
  destination: IssueCreationDestinationDTO | null;
  /** The GitHub account issues are created as, once connected; `null` for native. */
  githubLogin: string | null;
  /**
   * Accounts the repository's issues can be assigned to; empty if unread.
   * Native: the organization's members (login = email, with userId).
   */
  assignableUsers: ExternalIssueUserDTO[];
  /** Why the target could not be read, for `unavailable`. */
  error: {
    issueError: ExternalIssueErrorCode | string;
    message: string;
  } | null;
}

/** POST /api/v1/orgs/:orgId/issues */
export interface CreateIssueInput {
  title: string;
  body: string;
  /**
   * GitHub logins, or members' emails for the native tracker; `@me` is the
   * caller's connected account (native: the caller).
   */
  assignees: string[];
  /** Stable across checks and retries of one submission. */
  idempotencyKey: string;
  /** The destination the person saw; a changed target is refused. */
  domainId: string;
  /** Only look for an issue this submission already created; never send. */
  checkOnly?: boolean;
  /**
   * The issue to add the new one under as a GitHub sub-issue: a GitHub issue
   * URL, `owner/name#N`, `#N` or `N` (a number in the destination). It must
   * be an issue in the destination repository. Omitted for none.
   */
  parent?: string;
  /**
   * Label names from the destination domain's catalog (at most 20);
   * omitted for none.
   */
  labels?: string[];
}

/** The issue a creation produced or found. */
export interface CreatedIssueDTO {
  externalIssueId: string;
  repositoryId: string;
  number: number;
  /** `owner/name#number` or `ACME-12`, the reference Start workspace accepts. */
  displayKey: string;
  title: string;
  url: string;
}

export interface IssueCreationDTO {
  /**
   * `created` (the issue exists), `outcome_unknown` (GitHub gave no definite
   * answer; check before creating again) or `not_found` (a check found no
   * issue from this submission).
   */
  state: "created" | "outcome_unknown" | "not_found";
  /** This request sent the issue; `false` when an earlier one was found. */
  sent: boolean;
  issue: CreatedIssueDTO | null;
  idempotencyKey: string;
  destination: IssueCreationDestinationDTO;
  /** `null` for a native issue. */
  githubLogin: string | null;
  title: string;
  /** The exact description sent, including G5's hidden reference line. */
  payload: string;
  /** The logins sent, with `@me` resolved. */
  assignees: string[];
  /** Why the outcome is unknown. */
  message: string | null;
  /** The requested parent issue; `null` when none was requested. */
  parent: CreationParentDTO | null;
  /** The labels requested. */
  labels: string[];
  /**
   * Requested labels GitHub's answer lacks: it silently drops labels the
   * author may not set. The issue exists; they can be added from its page.
   */
  labelsNotApplied?: string[];
}

/**
 * POST /api/v1/workspaces/:workspaceId/issue-links/:linkId/state — a person
 * closes (as completed) or reopens a linked issue as their own GitHub
 * account (g5.external-issues.transition). Nothing is recorded; the write is
 * idempotent.
 */
export type IssueStateChangeInput = {
  linkId: string;
  state: "closed" | "open";
};

export interface IssueStateChangeDTO {
  requested: "closed" | "open";
  /** `unknown`: GitHub gave no definite answer; read the issue or send again. */
  outcome: "applied" | "unknown";
  /** The issue as GitHub answered, for `applied`. */
  issue: ExternalIssueSummaryDTO | null;
  /** `null` for a native issue. */
  githubLogin: string | null;
  message: string | null;
}

/**
 * POST …/issues/g5/:number/comments or
 * …/issues/github/:repositoryId/:number/comments (the issue page).
 */
export interface IssuePageCommentInput {
  body: string;
  /** Stable across retries of one submission. */
  idempotencyKey: string;
}

/** PATCH …/issues/{g5/:number | github/:repositoryId/:number}/comments/:commentId */
export interface EditIssuePageCommentInput {
  commentId: string;
  body: string;
  /** The comment's `updatedAt` as the editor read it. */
  expectedUpdatedAt: string;
}

/**
 * An image attached to a G5 issue (g5.issue.native.attachments):
 * POST /api/v1/orgs/:orgId/issues/g5/:number/attachments.
 */
export interface NativeAttachmentDTO {
  id: string;
  fileName: string;
  /** The type G5 recognized from the content (PNG, JPEG, GIF or WebP). */
  contentType: string;
  sizeBytes: number;
  sha256: string;
  uploader: ExternalIssueUserDTO;
  /** `agent` when the workspace's agent uploaded it as its owner. */
  origin: "person" | "agent";
  createdAt: string;
  /** Its address under the issue's page, which text shows it by. */
  url: string;
  /** `![<fileName>](<url>)`, for a composer to insert. */
  markdown: string;
}

/** GET …/attachments: where each first appears, derived from the text. */
export interface NativeAttachmentListItemDTO extends NativeAttachmentDTO {
  /** `issue` when no text shows it. */
  place: "description" | "comment" | "issue";
  commentId: string | null;
}

export interface UploadNativeAttachmentInput {
  fileName: string;
  contentBase64: string;
  /** Stable across retries of the same upload. */
  idempotencyKey: string;
}

/**
 * PATCH /api/v1/orgs/:orgId/issues/g5/:number — only the fields given
 * change; applies only to the version the editor saw.
 */
export interface EditNativeIssueInput {
  title?: string;
  body?: string;
  /** Members' emails. */
  assignees?: string[];
  /** The issue's `updatedAt` as the editor read it. */
  expectedUpdatedAt: string;
}

/**
 * GET /api/v1/orgs/:orgId/issues/github/:repositoryId/:number/write-target —
 * whether the viewer can change a GitHub issue from its page, and as which
 * GitHub account. `state` follows the comment composer's.
 */
export interface IssueWriteTargetDTO {
  state: CommentComposerState;
  /** The connected GitHub account writes are made as; `null` until connected. */
  githubLogin: string | null;
  /** An untrusted domain reviews exactly what is sent before writing. */
  contentTrusted: boolean;
  displayKey: string;
  destination: CommentDestinationDTO;
  /** Accounts the repository's issues can be assigned to; empty if unread. */
  assignableUsers: ExternalIssueUserDTO[];
  /** Why the target could not be read, for `unavailable`. */
  error: {
    issueError: ExternalIssueErrorCode | string;
    message: string;
  } | null;
}

/** The fields of a GitHub issue an edit changes. */
export interface GithubIssueFields {
  title?: string;
  body?: string;
  /** GitHub logins: the complete new set. */
  assignees?: string[];
}

/**
 * PATCH /api/v1/orgs/:orgId/issues/github/:repositoryId/:number — only the
 * fields given change. `seen` holds what the form showed for each changed
 * field; GitHub's current value differing from it is refused (409
 * `issue_changed`).
 */
export interface EditGithubIssueInput extends GithubIssueFields {
  seen: GithubIssueFields;
}

/** An issue edit's result (g5.external-issues.edit). */
export interface IssueEditDTO {
  /**
   * `applied`; `partial` (GitHub applied the write but its answer lacks a
   * requested assignee change); `unknown` (no definite answer: saving the
   * same values again is harmless).
   */
  outcome: "applied" | "partial" | "unknown";
  /** The issue as the tracker answered, unless `unknown`. */
  issue: ExternalIssueSummaryDTO | null;
  /** `null` for a native issue. */
  githubLogin: string | null;
  /** Logins GitHub's answer does not list although they were added. */
  notAssigned?: string[];
  /** Logins GitHub's answer still lists although they were removed. */
  notUnassigned?: string[];
  message: string | null;
}

/** The parent a creation asked for, and whether the issue is under it. */
export interface CreationParentDTO {
  /** GitHub REST issue id, decimal string. */
  externalIssueId: string;
  number: number;
  /** `owner/name#N`. */
  displayKey: string;
  title: string;
  url: string;
  /**
   * `pending`: no issue exists yet (state not_found / outcome_unknown);
   * `attached`: the issue is a sub-issue of this parent;
   * `not_attached`: a check found it without a parent, or GitHub refused
   * (see message);
   * `outcome_unknown`: GitHub gave no definite answer, or the issue's parent
   * could not be read; retrying with the same key retries the attach;
   * `other_parent`: the issue is already a sub-issue of another issue; G5
   * never moves it.
   */
  state:
    | "pending"
    | "attached"
    | "not_attached"
    | "outcome_unknown"
    | "other_parent";
  message: string | null;
}

/** GET /api/v1/workspaces/:workspaceId/issue-links */
export interface WorkspaceIssueLinkListDTO {
  links: WorkspaceIssueLinkDTO[];
}

/** POST /api/v1/workspaces/:workspaceId/issue-links */
export interface CreateWorkspaceIssueLinkInput {
  ref: string;
  role: WorkspaceIssueLinkRole;
}

/** Instance info from GET /api/v1/instance/info */
export interface InstanceInfoDTO {
  version: string; // Semver, e.g., '0.1.0'
  features: string[]; // Feature flags such as 'chat' and UI capability gates.
  authProviders: string[]; // configured platform logins: 'github', 'microsoft'
  maxUploadSizeMb: number;
  selfHosted: boolean; // true for customer self-hosted deployments
  devLoginEnabled: boolean; // true when G5_E2E_AUTH=true (dev/demo login)
  motd?: string; // Optional message of the day
}

/** Structured client context for a platform bug report. camelCase keys mirror
 *  the server DTO and are rendered into the GitHub issue body. */
export interface PlatformIssueContext {
  route?: string;
  appVersion?: string;
  userAgent?: string;
  viewport?: string;
  workspaceSlug?: string;
  orgSlug?: string;
  breadcrumbs?: string[];
}

/** Input to POST /api/v1/platform/issues (report a bug about the G5 platform). */
export interface PlatformIssueReportInput {
  title: string;
  body: string;
  kind?: string; // defaults to "bug" server-side
  severity: string;
  context: PlatformIssueContext;
  /** Optional kept screenshot (PNG blob). */
  screenshot?: Blob | null;
}

/** Acknowledgement from POST /api/v1/platform/issues (write-only v1). */
export interface PlatformIssueReportResult {
  submissionId: string;
}

/** Chat model from GET /api/v1/instance/chat/models */
export interface ChatModelDTO {
  id: string;
  name: string;
  provider: string;
  contextWindow: number | null;
  maxOutputTokens: number | null;
  supportsStreaming: boolean;
  costPer1kInput: number | null;
  costPer1kOutput: number | null;
  agentProvider: ChatAgentProvider;
  route: "direct" | "openrouter";
  efforts: string[];
  defaultEffort: string;
  modes: string[];
  cliVersion: string;
}

/** Health status from GET /api/v1/instance/health */
export interface HealthStatus {
  status: string; // 'healthy' | 'degraded' | 'unhealthy'
  checks: {
    database: string; // 'healthy' | 'unhealthy'
  };
  uptime: number; // seconds (Rust field is `uptime`, not `uptimeSeconds`)
}

/** Spend history from GET /api/v1/workspaces/:workspaceId/settings/spend/history */
export interface SpendHistoryDTO {
  granularity: string;
  periods: Array<{
    periodStart: string;
    periodEnd: string;
    totalSpendUsd: number;
    totalInputTokens: number;
    totalOutputTokens: number;
    taskCount: number;
  }>;
  daily: Array<{
    date: string;
    spendUsd: number;
    inputTokens: number;
    outputTokens: number;
  }>;
}

// ---------------------------------------------------------------------------
// 19. Legacy Mock Types (V1 compatibility)
// ---------------------------------------------------------------------------

/** Repository — multi-repo workspace support (V1 mock model) */
export interface Repo {
  id: string;
  org: string;
  name: string;
  branches: Branch[];
}

/** Branch (V1 mock model) */
export interface Branch {
  name: string;
  isDefault: boolean;
}

/** Workspace (V1 mock model — simplified) */
export interface Workspace {
  id: string;
  name: string;
  status: "active" | "blocked" | "review" | "integrated" | "idea";
  description: string;
  repos: { repoId: string; branch: string }[];
}

/** Chat message (V1 mock model — simplified) */
export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  tools?: string[];
}

/** Chat session (V1 mock model) */
export interface ChatSession {
  id: string;
  title: string;
  preview: string;
  time: string;
  isActive: boolean;
}

export interface StartWorkspaceSyncChatResponse {
  sessionId: string;
  assistantMessageId: string;
  taskId: string;
  targetCount: number;
}

// Generate button option contract (g5-common core D32). Wire form mirrors
// the Rust `GenerateOptionsV1` camelCase serde shape exactly.
export type GenerateCodeMode = "stubs" | "full" | "none";
export type GenerateExplainerStyle = "concise" | "balanced" | "indepth";
export type GenerateExplainerFormat = "page" | "slides";
export type GenerateAutonomy = "raise_all" | "balanced" | "autonomous";

export interface GenerateExplainerOptionsV1 {
  enabled: boolean;
  style: GenerateExplainerStyle;
  format: GenerateExplainerFormat;
}

export interface GeneratePlansV1 {
  ontology: boolean;
  designDoc: boolean;
  questionDoc: boolean;
  implPlan: boolean;
  testPlan: boolean;
  explainer: GenerateExplainerOptionsV1;
  mocks: boolean;
}

export interface GenerateReportsV1 {
  review: boolean;
  coverage: boolean;
  statusOverview: boolean;
}

export interface GenerateOptionsV1 {
  plans: GeneratePlansV1;
  code: GenerateCodeMode;
  reports: GenerateReportsV1;
  autonomy: GenerateAutonomy;
  /** Reserved: the auto-PR motion is deferred; true is rejected server-side. */
  autoPullRequest?: boolean;
}

/** Modal defaults of record (generate-button design §0). */
export const defaultGenerateOptions = (): GenerateOptionsV1 => ({
  plans: {
    ontology: true,
    designDoc: true,
    questionDoc: true,
    implPlan: true,
    testPlan: true,
    explainer: { enabled: true, style: "balanced", format: "page" },
    mocks: true,
  },
  code: "full",
  reports: { review: true, coverage: true, statusOverview: true },
  autonomy: "autonomous",
});

/** A committed file that can be launched as a downlift sync task. */
export interface PendingSyncFile {
  id: string;
  targetKind: "doc";
  path: string;
  repoId: string;
  repoName: string;
  branchName: string;
  headCommit: string;
  lastDownliftedCommitSha: string | null;
  pendingReasons?: Array<"document_change">;
  targetDocRef?: string;
}

export type PendingSyncDocFile = Omit<
  PendingSyncFile,
  "targetKind" | "targetDocRef"
> & {
  targetKind: "doc";
  targetDocRef: string;
};

/**
 * Client mirror of the server's Project-Docs-only pending sync scope
 * (defense in depth): isolated sync rows must live in the current
 * workspace's dedicated docs folder `docs/<workspaceId>/`. The server
 * pending-sync computation is authoritative; this filter keeps stale cached
 * rows from being rendered, counted, or submitted.
 */
export function isIsolatedPendingSyncDocFile(
  file: PendingSyncFile,
  workspaceId: string,
): file is PendingSyncDocFile {
  const lowerPath = file.path.toLowerCase();
  const projectDocsPrefix = `docs/${workspaceId.toLowerCase()}/`;
  if (!lowerPath.startsWith(projectDocsPrefix)) {
    return false;
  }
  const projectDocsRelativePath = file.path.slice(projectDocsPrefix.length);
  const isLegacyMergeReport = /^merge\/[^/]+_merge_report\.md$/.test(
    projectDocsRelativePath,
  );
  const isReservedWorkflowArtifact =
    projectDocsRelativePath.startsWith("sync/") ||
    projectDocsRelativePath === "code_review.md" ||
    isLegacyMergeReport;
  return (
    file.targetKind === "doc" &&
    typeof file.targetDocRef === "string" &&
    file.targetDocRef.length > 0 &&
    !isReservedWorkflowArtifact &&
    (lowerPath.endsWith(".md") ||
      lowerPath.endsWith(".html") ||
      lowerPath.endsWith(".htm") ||
      lowerPath.endsWith(".jsonl"))
  );
}

// ---------------------------------------------------------------------------
// 20. Human-in-the-Loop (HIL) Escalation Types
// ---------------------------------------------------------------------------

/** Severity of the agent's uncertainty that triggered escalation */
export type UncertaintySeverity = "low" | "medium" | "high" | "critical";

/** A suggested option the agent presents to the human */
export interface EscalationOption {
  label: string;
  description: string;
  tradeoffs: string | null;
  risks: string | null;
  recommended: boolean;
}

// ---------------------------------------------------------------------------
// Remote Repo Tracking — sync runs panel (G1)
// ---------------------------------------------------------------------------

/**
 * One row of the `branch_sync_runs` audit table. Mirrors
 * `BranchSyncRunDTO` in `g5-api-routes/src/routes/workspaces.rs`.
 */
export interface BranchSyncRun {
  id: string;
  workspaceId: string;
  repoId: string;
  actingUserId: string;
  /** `'integration' | 'advisory'`. */
  kind: string;
  /** `'commit' | 'manual' | 'workspace_open' | 'ping' | 'webhook' | 'background_inbound'`. */
  triggerKind: string;
  /**
   * `'noop' | 'fetched' | 'imported' | 'pushed' | 'diverged' | 'dirty_skipped' |
   *  'advisory_updated' | 'auth_required' | 'push_failed' | 'fetch_failed' |
   *  'resolved'`.
   */
  status: string;
  oldWorkspaceTip?: string;
  newWorkspaceTip?: string;
  oldMirrorTip?: string;
  newMirrorTip?: string;
  newBaseline?: string;
  errorDetail?: string;
  mergeRunId?: string;
  startedAt: string;
  completedAt?: string;
}

/**
 * Tracking summary for one canonical workspace repository member.
 */
export interface BranchTrackingRepo {
  repoId: string;
  repoName: string;
  repoBranchName: string;
  trackingMode: string;
  headCommit: string;
  remoteBranch?: string;
  baselineCommit?: string;
  /**
   * Non-null when the integration cycle is paused. Known values:
   * `'diverged'`, `'watermark_violation'`, `'auth_required'`,
   * `'push_failed'`, `'fetch_failed'`, `'merge_run:<id>'`. The UI maps
   * these to human-readable strings.
   */
  syncPausedReason?: string;
  lastSyncAt?: string;
  remoteTipAdvisorySha?: string;
  remoteTipAdvisoryObservedAt?: string;
  /** Most recent `branch_sync_runs.status` for this child row. */
  latestRunStatus?: string;
  /** The half-merged workspace PR holding this repository's auto-pull. */
  importHeldBy?: ImportHold;
  /** An accepted workspace rewrite awaits publication to the provider. */
  rewritePending?: boolean;
}

/** Branch-scoped tracking summary returned alongside the sync-runs feed. */
export interface BranchTrackingSummary {
  workspaceId: string;
  repos: BranchTrackingRepo[];
}

/** Response shape of `GET .../branches/:branchId/sync_runs`. */
export interface ListBranchSyncRunsResponse {
  branch: BranchTrackingSummary;
  runs: {
    data: BranchSyncRun[];
    total: number;
    page: number;
    perPage: number;
  };
}

/** Response shape of `POST .../branches/:branchId/sync_runs/retry`. */
export interface RetryBranchSyncRunsResponse {
  enqueued: number;
  /** `'reenqueued' | 'nothing' | 'non_retryable'`. */
  outcome: string;
}

/** Per-workspace outbound push mode (design §6.2, arch D66). */
export type PushMode = "manual" | "auto";

/** Response shape of `POST .../workspaces/:id/push_to_remote`. */
export interface PushToRemoteResponse {
  /** Count of remote-tracked branches enqueued for an explicit push. */
  enqueued: number;
  coordinated?: CoordinatedOperationResult;
}

/** Stable outcomes of `POST .../remote_tracking/approved_rebase_publish`. */
export type ApprovedRebasePublishStatus =
  | "queued"
  | "queued_partial"
  | "approval_checkpoint_refreshed";

/** Response shape of `POST .../remote_tracking/approved_rebase_publish`. */
export interface ApprovedRebasePublishResponse {
  enqueued: number;
  coordinated?: Omit<CoordinatedOperationResult, "operation" | "status"> & {
    operation: "approved_rebase_publish";
    status: ApprovedRebasePublishStatus;
  };
}

/**
 * `POST .../repos/:repoId/provider_history_preview`: what adopting a
 * repository's provider history would do, on Base
 * (g5.workspace.remote.base-provider-rewrite) or on a workspace paused for
 * divergence (g5.workspace.remote.adopt-provider-history).
 */
export interface ProviderHistoryPreview {
  workspaceId: string;
  repoId: string;
  remoteBranch: string;
  /** The workspace's current head, which adopting replaces. */
  head: string;
  /** The provider tip the workspace would adopt. */
  providerTip: string;
  /** Local commits the provider tip does not contain; adopting discards them. */
  discardedCommits: number;
  /**
   * The discarded commits are only the repository's initial (root) commit,
   * so adopting is the recommended reconciliation.
   */
  initialOnly?: boolean;
  syncPausedReason?: string | null;
}

/** Response shape of `POST .../repos/:repoId/accept_provider_history`. */
export interface AcceptProviderHistoryResponse {
  workspaceId: string;
  repoId: string;
  head: string;
}

/** Response shape of `PUT .../workspaces/:id/push_mode`. */
export interface SetPushModeResponse {
  pushMode: PushMode;
}

/**
 * Option 1 (G5 #1198): input for
 * `PUT .../workspaces/:id/branches/:branchId/repos/:repoId/auto_pull`.
 */
export interface SetBranchRepoAutoPullInput {
  repoId: string;
  enabled: boolean;
}

/** Response shape of the per-child auto-pull toggle. */
export interface SetBranchRepoAutoPullResponse {
  workspaceId: string;
  repoId: string;
  autoPullEnabled: boolean;
}

/**
 * Option 2 (G5 #1198): input for
 * `POST .../workspaces/:id/branches/:branchId/repos/:repoId/publish`.
 */
export interface PublishBranchRepoInput {
  repoId: string;
}

/** Response shape of the explicit Publish of a local-only branch child. */
export interface PublishBranchRepoResponse {
  workspaceId: string;
  repoId: string;
  remoteBranch: string;
  headCommit: string;
}

/** Bounded workspace task list entry; detail is fetched directly by task ID. */
export interface TaskListEntry {
  id: string;
  name: string;
  taskType: string;
  status: TaskStatus;
  workspaceId: string;
  sessionId: string | null;
  costUsd: number;
  createdAt: string;
  updatedAt: string;
  /** Why a failed task failed, from its recorded outcome; absent otherwise. */
  failure?: TaskFailure;
}

/** A failed task's reason code and, when recorded, its text detail. */
export interface TaskFailure {
  reason: string;
  detail?: string;
}

export interface TaskCostBreakdown {
  taskId: string;
  taskName: string;
  taskType: string;
  costUsd: number;
}
// Workspace landing, published revision vectors and requested review flow.
export type ReviewHead = {
  repoId: string;
  workspaceId: string;
  branchName: string;
  headCommit: string;
};
export type ReviewRequest = {
  note?: string | null;
  allowSelfApproval?: boolean;
  id: string;
  definitionId: string;
  definitionVersion: number;
  name: string;
  phase: WorkspacePhase;
  actorUserId: string;
  requestedBy: string;
  state: string;
  createdAt: string;
  closedAt: string | null;
};
export type ReviewDecision = {
  id: string;
  requestId: string;
  definitionId: string;
  name: string;
  phase: WorkspacePhase;
  actorUserId: string;
  outcome: string;
  comment: string | null;
  reviewedGitHeads: ReviewHead[];
  createdAt: string;
};
export type ReviewOverview = {
  workspace: {
    ownerUserId: string;
    targetWorkspaceId: string | null;
    phase: WorkspacePhase;
    readOnly: boolean;
  };
  pr: {
    id: string;
    title: string;
    description: string | null;
    sourceHeads: ReviewHead[];
    createdAt: string;
  } | null;
  heads: ReviewHead[];
  baseHeads: ReviewHead[] | null;
  parentHeads: ReviewHead[] | null;
  pushedAt: Record<string, string>;
  repos: { id: string; name: string; role: string; rootPath: string }[];
  requests: ReviewRequest[];
  decisions: ReviewDecision[];
};
export type ReviewComment = {
  id: string;
  workspaceId: string;
  authorId: string;
  authorName: string;
  scope: string;
  content: string;
  repoId?: string | null;
  filePath?: string | null;
  lineNumber?: number | null;
  commitSha?: string | null;
  phase?: WorkspacePhase | null;
  createdAt: string;
  resolved: boolean;
};
export type VectorRevision = {
  id: string;
  workspaceId: string;
  observedAt: string;
  heads: ReviewHead[];
  pushedAt: Record<string, string>;
};
export type ReviewFile = {
  repoId: string;
  path: string;
  kind?: string;
  status: string;
  additions: number | null;
  deletions: number | null;
};
export type ReviewChanges = {
  unavailableRepositories?: {
    repoId: string;
    repoName?: string;
    reason: string;
  }[];
  beforeHeads: ReviewHead[] | null;
  afterHeads: ReviewHead[];
  baselineAvailable: boolean;
  files: ReviewFile[];
  file: {
    repoId: string;
    path: string;
    binary?: boolean;
    before: string | null;
    after: string | null;
    beforeCommit: string | null;
    afterCommit: string | null;
  } | null;
};
