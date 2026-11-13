// =============================================================================
// MSW Baseline Handlers — API Mock Handlers for All g5-api Endpoints
// =============================================================================
// Exports a `handlers` array containing MSW request handlers for every g5-api
// endpoint the client calls. Each handler returns a happy-path response using
// mock data from the fixtures/mock-data modules.
//
// All handler responses wrap data in the g5-api response envelope format:
//   { data: T } for single entities
//   { data: T[] } for lists
//   { data: { items: T[], cursor: null, hasMore: false } } for paginated
//
// Individual tests override specific handlers via server.use(...) to simulate
// error states, empty responses, or custom data shapes.
// =============================================================================

import { HttpResponse, http } from "msw";

import type {
  ApiKey,
  BaseReverificationStatus,
  BranchSummary,
  CloneWorkspaceInput,
  CloneWorkspaceResponse,
  ReverificationHead,
  ReverificationPullRequest,
  ReverificationRun,
  WorkerProfile,
  WorkspaceBranchDetail,
  WorkspaceDetail,
  WorkspaceSummary,
} from "../../lib/types";
import { VERIFICATION_ACTION_ENABLED_SETTING } from "../../lib/verificationActionFlags";
import { DEFAULT_WORKER_PROFILE, WORKER_PROFILES } from "../../lib/types";
import { CLAUDE_CHAT_DEFAULT_MODEL } from "../../lib/claudeModels";
import { WORKBENCH_ACTIONS } from "../../lib/workbenchDefaults";
import {
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
  MOCK_NOTIFICATIONS,
  MOCK_ORGS,
  MOCK_ORG_INVITATIONS,
  MOCK_ORG_MEMBERS,
  MOCK_SPEND_HISTORY,
  MOCK_TASKS,
  MOCK_TASK_TIMELINES,
  MOCK_WORKSPACES,
  MOCK_WORKSPACE_DETAILS,
  MOCK_PERSONAL_WORKSPACE_DETAIL,
  MOCK_WORKSPACE_SETTINGS,
  MOCK_WORKSPACE_SPEND,
  getMockFileContent,
} from "../../data/mock";

import {
  MOCK_JWT,
  MOCK_WORKSPACE_ACTIVITY,
  createMockWorkspaceVm,
} from "../fixtures";

// ---------------------------------------------------------------------------
// Helper: Get first key value from a Record, or fallback
// ---------------------------------------------------------------------------

function firstValue<T>(record: Record<string, T>): T | undefined {
  const keys = Object.keys(record);
  return keys.length > 0 ? record[keys[0]] : undefined;
}

function getOrFirst<T>(record: Record<string, T>, key: string): T | undefined {
  return record[key] ?? firstValue(record);
}

// Membership activity is separate from role and historical attribution.
let mockOrgMembers = structuredClone(MOCK_ORG_MEMBERS);
export function resetMockOrgMembers(): void {
  mockOrgMembers = structuredClone(MOCK_ORG_MEMBERS);
}

function mockWorkspaceBranch(
  workspaceId: string,
  branch: BranchSummary,
): WorkspaceBranchDetail {
  return {
    workspaceId,
    repoId: branch.repoId,
    repoName: branch.repoName,
    branchName: branch.branchName,
    headCommit: branch.headCommit,
    trackingMode: branch.trackingMode,
    materializationStatus: "ready",
    hasDraft: branch.hasDraft,
    dirtyFileCount: branch.dirtyFileCount,
    dirtyFiles: [],
  };
}

function hasDisabledTestAuth(): boolean {
  return globalThis.localStorage?.getItem("g5_test_disable_auth") === "1";
}

function getStoredTestJwt(): string | null {
  return globalThis.localStorage?.getItem("g5_auth_token") ?? null;
}

function getStoredTestUser() {
  const raw = globalThis.localStorage?.getItem("g5_test_current_user");
  if (!raw) {
    return null;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<typeof MOCK_CURRENT_USER> & {
      scopes?: string[];
    };
    return {
      ...MOCK_CURRENT_USER,
      ...parsed,
      // Mocked users name only tenant orgs unless they set `orgKind`.
      orgs: (parsed.orgs ?? MOCK_CURRENT_USER.orgs).map((membership) => ({
        ...membership,
        orgKind: membership.orgKind ?? "tenant",
      })),
      scmConnections: parsed.scmConnections ?? MOCK_CURRENT_USER.scmConnections,
    };
  } catch {
    return null;
  }
}

function slugifyWorkspaceName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function slugifyOrgName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

// ---------------------------------------------------------------------------
// Org approval-definition store (B6 governance PR-1)
// ---------------------------------------------------------------------------
// Module-scope mutable store backing the org approval-definition handlers.
// POST create inserts v1; POST versions bumps the latest version in place
// (the wire DTO only ever carries the latest version). Tests reset it via
// resetMockOrgApprovalDefinitions() in beforeEach; conflict/validation cases
// override the handlers with server.use().

let mockOrgApprovalDefinitionStore: Record<
  string,
  import("../../lib/types").OrgApprovalDefinitionDTO[]
> = {};
let mockOrgApprovalDefinitionSeq = 0;

/** Reset the approval-definition store. Call in test beforeEach. */
export function resetMockOrgApprovalDefinitions(): void {
  mockOrgApprovalDefinitionStore = {};
  mockOrgApprovalDefinitionSeq = 0;
}

/** Seed a definition row directly (library-view tests). */
export function seedMockOrgApprovalDefinition(
  orgId: string,
  definition: import("../../lib/types").OrgApprovalDefinitionDTO,
): void {
  (mockOrgApprovalDefinitionStore[orgId] ??= []).push(definition);
}

function approvalHumansFromIds(
  orgId: string,
  humanUserIds: string[],
): import("../../lib/types").OrgApprovalHumanDTO[] {
  const members = getOrFirst(mockOrgMembers, orgId) ?? [];
  return humanUserIds.map((userId) => {
    const member = members.find((m) => m.userId === userId);
    return {
      userId,
      displayName: member?.displayName ?? "Unknown user",
      email: member?.email ?? "",
      avatarUrl: member?.avatarUrl ?? null,
      stillMember: !!member,
    };
  });
}

// ---------------------------------------------------------------------------
// Workspace approvals store (B6 governance PR-2)
// ---------------------------------------------------------------------------
// Mutable per-workspace state backing the workspace-approvals handlers.
// Attach/retire append events and recompute the active set; decisions append
// to the decision log and flip approver state. Tests seed via
// seedMockWorkspaceApprovals and reset via resetMockWorkspaceApprovals.

let mockWorkspaceApprovalsStore: Record<
  string,
  import("../../lib/types").WorkspaceApprovalsStateDTO
> = {};

export function resetMockWorkspaceApprovals(): void {
  mockWorkspaceApprovalsStore = {};
}

export function seedMockWorkspaceApprovals(
  workspaceId: string,
  state: import("../../lib/types").WorkspaceApprovalsStateDTO,
): void {
  mockWorkspaceApprovalsStore[workspaceId] = state;
}

function workspaceApprovalsState(
  workspaceId: string,
): import("../../lib/types").WorkspaceApprovalsStateDTO {
  return (mockWorkspaceApprovalsStore[workspaceId] ??= {
    requiredInto: [],
    attachmentHistory: [],
    outgoing: null,
  });
}

/**
 * The real server always projects `outgoing.mergeAdmission` (only the
 * Integration Engineering entries gate, g5.governance.approval.requirements).
 * A seeded state without one gets the same projection on the wire, so tests
 * seed rows and read the verdict the way the client does in production.
 */
function withServerAdmission(
  state: import("../../lib/types").WorkspaceApprovalsStateDTO,
): import("../../lib/types").WorkspaceApprovalsStateDTO {
  if (!state.outgoing || state.outgoing.mergeAdmission) return state;
  const requirements = state.outgoing.requirements.map((entry) => ({
    definitionId: entry.definition.id,
    definitionName: entry.definition.name,
    definitionVersion: entry.definition.latestVersion ?? 1,
    state: entry.state as "approved" | "rejected" | "pending",
  }));
  const blocking = requirements.filter((r) => r.state !== "approved");
  return {
    ...state,
    outgoing: {
      ...state.outgoing,
      mergeAdmission: {
        admitted: blocking.length === 0,
        reason:
          blocking.length === 0 ? null : "Approval(s) required before merging",
        evaluatedAt: new Date().toISOString(),
        requirements,
      },
    },
  };
}

// ---------------------------------------------------------------------------
// Worker-Host store (ORG-scoped cloud VM hosts)
// ---------------------------------------------------------------------------
// Module-scope mutable store backing the GET/POST/DELETE worker-host handlers.
// It models the real async lifecycle: POST inserts a `provisioning` row; after
// a couple of GET polls the row flips to `ready` (stands in for the server-side
// reconciler); DELETE transitions the row to `terminated` (never deletes it).
// Tests reset it via resetMockWorkerHosts() in beforeEach for isolation.

interface MockWorkerHostRow {
  hostId: string;
  orgId: string;
  instanceType: string;
  region: string;
  status: "provisioning" | "ready" | "unreachable" | "draining" | "terminated";
  createdAt: string;
  lastHeartbeat?: string;
  /** GET-list polls observed since insertion; drives provisioning→ready. */
  pollsSinceInsert: number;
  // Runtime/profile contract (PR1/PR2/PR3). `workerProfile` is always present
  // (default `fat-default`); `runtimeLabels` is always present on the wire
  // (default `{}`). The rest are absent-when-unset, and `runtimeSchema` is null
  // for the absent-schema success state.
  baseImageRef?: string;
  baseImageDigest?: string;
  baseImageSourceKind?: "track" | "pin";
  baseImageSourceValue?: string;
  runtimeSchema?: string | null;
  runtimeLabels?: Record<string, string>;
  runtimeLabelsRefreshedAt?: string;
  workerProfile?: WorkerProfile;
}

let mockWorkerHostStore: MockWorkerHostRow[] = [];
let mockWorkerHostSeq = 0;

/**
 * Per-org default base image policy plus the validated deployment catalog.
 * The catalog is public only through the org-authorized worker-host envelope;
 * hosts always persist the resolved immutable ref and source provenance.
 */
const mockOrgDefaultBaseImageRefs = new Map<string, string>();
const mockOrgDefaultImageTracks = new Map<string, string>();
const MOCK_PROMOTED_IMAGE_REF = `reg.example.com/g5/worker-current@sha256:${"c".repeat(64)}`;
const MOCK_PROMOTED_IMAGE_TRACKS = [
  {
    name: "current",
    imageRef: MOCK_PROMOTED_IMAGE_REF,
    digest: `sha256:${"c".repeat(64)}`,
  },
];

/** Pin an org's default base image the way a release pin migration does. */
export function setMockOrgDefaultBaseImageRef(
  orgId: string,
  ref: string,
): void {
  mockOrgDefaultBaseImageRefs.set(orgId, ref);
  mockOrgDefaultImageTracks.delete(orgId);
}

/**
 * Number of GET list polls a `provisioning` host is observed before the mock
 * reconciler flips it to `ready`. The first poll keeps it `provisioning` (so a
 * fresh provision is observably transient); the next poll readies it.
 */
/**
 * Representative server-published boot-disk policy. These literals are a test
 * fixture; production clients consume the API coefficients rather than owning
 * the rule (ontology `g5.runtime.placement.host-disk`).
 */
const MOCK_BOOT_DISK_SIZING = {
  baseGb: 120,
  perSlotGb: 80,
  floorGb: 200,
  minGb: 30,
  maxGb: 2000,
};

const MOCK_WORKER_HOST_READY_AFTER_POLLS = 2;
const BASE_IMAGE_REF_RE =
  /^[a-z0-9.-]+(?::[0-9]+)?\/[a-z0-9._/-]+@sha256:[0-9a-f]{64}$/;
const RUNTIME_SCHEMA_KEY = "com.g5labs.runtime.schema";
const STORED_RUNTIME_LABEL_ALLOWLIST = new Set([
  "com.g5labs.runtime.base_image",
  "com.g5labs.runtime.workload.class",
  "com.g5labs.runtime.kernel.features",
  "com.g5labs.runtime.testcontainers.enabled",
  "com.g5labs.runtime.devices",
  "com.g5labs.runtime.isolation",
  "com.g5labs.runtime.resources.cpu_min",
  "com.g5labs.runtime.resources.mem_min_mib",
]);

/** Reset the worker-host store. Call in test `beforeEach` for isolation. */
export function resetMockWorkerHosts(): void {
  mockWorkerHostStore = [];
  mockWorkerHostSeq = 0;
  mockOrgDefaultBaseImageRefs.clear();
  mockOrgDefaultImageTracks.clear();
}

/** Seed a pre-provenance host for legacy read/refresh compatibility tests. */
export function seedMockLegacyWorkerHostWithoutImage(orgId: string): string {
  mockWorkerHostSeq += 1;
  const hostId = `legacy-host-${mockWorkerHostSeq}`;
  mockWorkerHostStore.push({
    hostId,
    orgId,
    instanceType: "n2-standard-8",
    region: "us-central1",
    status: "ready",
    createdAt: new Date().toISOString(),
    pollsSinceInsert: 0,
    workerProfile: DEFAULT_WORKER_PROFILE,
    runtimeLabels: {},
  });
  return hostId;
}

/** Serialise a stored row to the camelCase wire DTO, omitting absent optionals. */
function workerHostToWire(row: MockWorkerHostRow): Record<string, unknown> {
  const wire: Record<string, unknown> = {
    hostId: row.hostId,
    orgId: row.orgId,
    instanceType: row.instanceType,
    region: row.region,
    status: row.status,
    createdAt: row.createdAt,
    // Always present on the wire (mirrors the backend serde contract).
    workerProfile: row.workerProfile ?? DEFAULT_WORKER_PROFILE,
    runtimeLabels: row.runtimeLabels ?? {},
  };
  if (row.lastHeartbeat !== undefined) {
    wire.lastHeartbeat = row.lastHeartbeat;
  }
  if (row.baseImageRef !== undefined) {
    wire.baseImageRef = row.baseImageRef;
  }
  if (row.baseImageDigest !== undefined) {
    wire.baseImageDigest = row.baseImageDigest;
  }
  if (row.baseImageSourceKind !== undefined) {
    wire.baseImageSourceKind = row.baseImageSourceKind;
  }
  if (row.baseImageSourceValue !== undefined) {
    wire.baseImageSourceValue = row.baseImageSourceValue;
  }
  // `runtimeSchema` is emitted only when non-null; null/absent ⇒ omitted (the
  // absent-schema success state where the image opted out of G5 metadata).
  if (row.runtimeSchema != null) {
    wire.runtimeSchema = row.runtimeSchema;
  }
  if (row.runtimeLabelsRefreshedAt !== undefined) {
    wire.runtimeLabelsRefreshedAt = row.runtimeLabelsRefreshedAt;
  }
  return wire;
}

/**
 * Derive a base image digest from a digest-pinned ref (`…@sha256:<hex>`). Returns
 * undefined for a tag-only/blank ref (the real backend resolves the digest on
 * its side; the mock just echoes the one already pinned in the ref).
 */
function deriveBaseImageDigest(ref: string | undefined): string | undefined {
  if (!ref) return undefined;
  const at = ref.lastIndexOf("@");
  if (at > 0 && ref.slice(at + 1).startsWith("sha256:")) {
    return ref.slice(at + 1);
  }
  return undefined;
}

function isWorkerProfile(value: string): value is WorkerProfile {
  return WORKER_PROFILES.includes(value as WorkerProfile);
}

function isDigestPinnedBaseImageRef(value: string): boolean {
  return BASE_IMAGE_REF_RE.test(value);
}

function filterRuntimeLabels(rawLabels: Record<string, string>): {
  runtimeSchema: string | null;
  runtimeLabels: Record<string, string>;
} {
  const rawSchema = rawLabels[RUNTIME_SCHEMA_KEY]?.trim();
  if (!rawSchema) {
    return { runtimeSchema: null, runtimeLabels: {} };
  }
  if (rawSchema !== "v1") {
    return { runtimeSchema: rawSchema, runtimeLabels: {} };
  }
  const runtimeLabels: Record<string, string> = {};
  for (const [key, value] of Object.entries(rawLabels)) {
    if (STORED_RUNTIME_LABEL_ALLOWLIST.has(key)) {
      runtimeLabels[key] = value;
    }
  }
  return { runtimeSchema: "v1", runtimeLabels };
}

// ---------------------------------------------------------------------------
// Runtime Config (org/workspace/user scoped) — in-memory mock state.
// Demo seed: token + base URL + Opus model at org; model overridden at workspace.
// ---------------------------------------------------------------------------

type MockScope = "org" | "workspace" | "user";

const mockScopedConfig: Record<
  MockScope,
  {
    settings: Record<string, unknown>;
    secretKeys: string[];
    secretHints: Record<string, string>;
  }
> = {
  org: {
    settings: {
      ANTHROPIC_BASE_URL: "https://openrouter.ai/api",
      ANTHROPIC_MODEL: CLAUDE_CHAT_DEFAULT_MODEL,
    },
    secretKeys: ["ANTHROPIC_AUTH_TOKEN"],
    // Baseline has no stored value for the seeded key, so no hint — matching
    // the server, which derives hints only from stored values (config route
    // D6). Tests exercising hint display seed hints explicitly.
    secretHints: {},
  },
  workspace: {
    settings: { ANTHROPIC_MODEL: CLAUDE_CHAT_DEFAULT_MODEL },
    secretKeys: [],
    secretHints: {},
  },
  user: { settings: {}, secretKeys: [], secretHints: {} },
};

/**
 * Mirror of the server's bounded secret display hint derivation
 * (config_service D9; security_policy secret_custody_and_redaction D9): first
 * 12 characters, only for values ≥ 24 chars, single-line (no Unicode
 * mandatory line break), not `{`/`[`-leading after trimming leading
 * whitespace and invisible formatting characters, and never for
 * CODEX_AUTH_JSON. Kept rule-for-rule in sync with
 * `ConfigService::secret_display_hint` so an MSW-backed test cannot pass
 * against a wider projection than production would ever return.
 */
function mockSecretDisplayHint(key: string, value: string): string | null {
  if (key.trim() === "CODEX_AUTH_JSON") {
    return null;
  }
  // Every Unicode mandatory line break, matching the server: LF, VT, FF, CR,
  // NEL, LS, PS.
  const lineBreaks = [
    "\n",
    "\r",
    "\u000B",
    "\u000C",
    "\u0085",
    "\u2028",
    "\u2029",
  ];
  if (lineBreaks.some((c) => value.includes(c))) {
    return null;
  }
  const chars = [...value];
  if (chars.length < 24) {
    return null;
  }
  // Trim leading whitespace plus invisible formatting characters (BOM,
  // zero-width space/joiners, word joiner), matching the server.
  const invisiblePrefix = ["\uFEFF", "\u200B", "\u200C", "\u200D", "\u2060"];
  let start = 0;
  while (
    start < chars.length &&
    (/\s/.test(chars[start]) || invisiblePrefix.includes(chars[start]))
  ) {
    start += 1;
  }
  const trimmed = chars.slice(start).join("");
  if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
    return null;
  }
  return chars.slice(0, 12).join("");
}

// Org admin settings saved through PUT, by org; unset orgs read the platform
// defaults. Reset after every test by `resetMockOrgAdminSettings` (setup.ts).
const DEFAULT_MOCK_ORG_ADMIN_SETTINGS = {
  defaultIdleTimeoutSecs: 28800,
  maxRunningVms: 10,
  maxRunningVmsPerOwner: 5,
  maxWorkspacesPerOwner: 50,
  autoAddHosts: true,
  autoHostInstanceType: "c3-standard-4",
  autoRetireIdleHosts: true,
  idleHostRetireSecs: 3600,
  minFreeSlots: 1,
};
const mockOrgAdminSettings = new Map<
  string,
  typeof DEFAULT_MOCK_ORG_ADMIN_SETTINGS
>();

function mockOrgAdminSettingsFor(orgId: string) {
  return mockOrgAdminSettings.get(orgId) ?? DEFAULT_MOCK_ORG_ADMIN_SETTINGS;
}

export function resetMockOrgAdminSettings(): void {
  mockOrgAdminSettings.clear();
}

// Workbench phases saved through PATCH, by workspace id, so a later detail
// read returns the chosen phase like the API (the mock app's chevrons stick).
// Reset after every test by `resetMockWorkspacePhases` (setup.ts).
const mockWorkspacePhases = new Map<string, "be" | "se" | "ie">();

function withMockWorkspacePhase<T extends object | undefined>(
  id: string,
  detail: T,
): T {
  const phase = mockWorkspacePhases.get(id);
  return phase && detail ? { ...detail, phase } : detail;
}

export function resetMockWorkspacePhases(): void {
  mockWorkspacePhases.clear();
}

// ---------------------------------------------------------------------------
// Base re-verification (g5.reverification) — demo status and run start.
// Demo seed: one completed full run and four pull requests merged since, so
// Home's line reads stale. A start through the clone route adds the run in
// progress and its workspace (listed, resolvable, openable). Reset after
// every test by `resetMockBaseReverification` (setup.ts).
// ---------------------------------------------------------------------------

const MOCK_REVERIFIED_HEADS: ReverificationHead[] = [
  {
    repoId: "repo-g5-client",
    rootPath: "g5-client",
    commit: "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c",
  },
  {
    repoId: "repo-g5-api",
    rootPath: "g5-api",
    commit: "9a8b7c6d5e4f30211203f4e5d6c7b8a99a8b7c6d",
  },
];
const MOCK_BASE_HEADS: ReverificationHead[] = [
  {
    repoId: "repo-g5-client",
    rootPath: "g5-client",
    commit: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
  },
  {
    repoId: "repo-g5-api",
    rootPath: "g5-api",
    commit: "b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1",
  },
];
const MOCK_REVERIFICATION_PULL_REQUESTS: ReverificationPullRequest[] = [
  {
    id: "pr-reverify-4",
    number: 1184,
    title: "Retry provider requests after a VM restart",
    mergedAt: "2026-11-05T16:20:00Z",
    mergedBy: "user-jordan",
  },
  {
    id: "pr-reverify-3",
    number: 1179,
    title: "Show approval gates on the Workspaces tile",
    mergedAt: "2026-11-04T10:05:00Z",
    mergedBy: "user-alex",
  },
  {
    id: "pr-reverify-2",
    number: 1171,
    title: "Keep the preview profile name while closing",
    mergedAt: "2026-11-02T14:40:00Z",
    mergedBy: "user-taylor",
  },
  {
    id: "pr-reverify-1",
    number: 1166,
    title: "Faster workspace fork for large repositories",
    mergedAt: "2026-10-30T09:15:00Z",
    mergedBy: "user-alex",
  },
];

function seedMockReverificationRuns(): ReverificationRun[] {
  return [
    {
      id: "run-reverify-1",
      kind: "full",
      state: "completed",
      workspaceId: "ws-base-reverification-2026-10-28",
      workspaceName: "base-reverification-2026-10-28",
      workspaceDeleted: true,
      ownerUserId: "user-jordan",
      startedBy: "user-jordan",
      startedAt: "2026-10-28T08:00:00Z",
      completedAt: "2026-10-29T17:30:00Z",
      label: "Base re-verification 2026-10-28 (full)",
      baseHeads: MOCK_REVERIFIED_HEADS,
      sinceHeads: null,
    },
  ];
}

let mockReverificationRuns = seedMockReverificationRuns();
let mockReverificationWorkspaces: WorkspaceSummary[] = [];

export function resetMockBaseReverification(): void {
  mockReverificationRuns = seedMockReverificationRuns();
  mockReverificationWorkspaces = [];
}

/** Every workspace the fixture knows, with the runs' forks after a start. */
function mockWorkspaceList(): WorkspaceSummary[] {
  return mockReverificationWorkspaces.length === 0
    ? MOCK_WORKSPACES
    : [...MOCK_WORKSPACES, ...mockReverificationWorkspaces];
}

function mockReverificationWorkspaceDetail(
  id: string,
): WorkspaceDetail | undefined {
  const summary = mockReverificationWorkspaces.find((w) => w.id === id);
  if (!summary) return undefined;
  const template = MOCK_WORKSPACE_DETAILS["task-graph-rework"];
  return {
    ...template,
    id: summary.id,
    name: summary.name,
    description: summary.description,
    clonedFromId: "base",
    createdBy: summary.createdBy,
    ownerUserId: summary.ownerUserId,
    createdAt: summary.createdAt,
    updatedAt: summary.lastUpdated,
    branches: summary.branches,
    docsFolderPath: `docs/${summary.id}`,
  };
}

function mockBaseReverificationStatus(): BaseReverificationStatus {
  // The server answers only "disabled" while the switch is off; unset reads
  // as on here, so a test or the preview that turns the switch on sees data.
  if (
    mockScopedConfig.org.settings[VERIFICATION_ACTION_ENABLED_SETTING] === false
  )
    return { enabled: false };
  const runs = [...mockReverificationRuns].sort((a, b) =>
    b.startedAt.localeCompare(a.startedAt),
  );
  const inProgress = runs.find((run) => run.state === "in_progress") ?? null;
  const lastCompleted = runs.find((run) => run.state === "completed") ?? null;
  const pullRequests = lastCompleted ? MOCK_REVERIFICATION_PULL_REQUESTS : [];
  return {
    enabled: true,
    baseWorkspaceId: "base",
    baseWorkspaceName: "base",
    state: inProgress
      ? "in_progress"
      : !lastCompleted
        ? "never"
        : pullRequests.length > 0
          ? "stale"
          : "current",
    canStart: true,
    lastCompleted,
    inProgress,
    pullRequestCount: pullRequests.length,
    pullRequests,
    lines: lastCompleted ? { added: 3410, removed: 1022 } : null,
    linesUnavailable: false,
    runs,
    runCount: runs.length,
  };
}

function reverificationRefusal(status: number, code: string, message: string) {
  return HttpResponse.json(
    { error: { code, message, requestId: "" } },
    { status },
  );
}

/**
 * A fork of Base that starts a re-verification run (g5.reverification.run).
 * Other clone requests are left to the test that makes them.
 */
async function mockStartBaseReverification(
  sourceId: string,
  request: Request,
): Promise<Response | undefined> {
  const input = (await request
    .clone()
    .json()
    .catch(() => null)) as CloneWorkspaceInput | null;
  const kind = input?.reverification?.kind;
  if (!input || !kind) return undefined;
  if (sourceId !== "base") {
    return reverificationRefusal(
      422,
      "REVERIFICATION_SOURCE_NOT_BASE",
      "Only Base can be re-verified.",
    );
  }
  const running = mockReverificationRuns.find(
    (run) => run.state === "in_progress",
  );
  if (running) {
    return reverificationRefusal(
      409,
      "REVERIFICATION_IN_PROGRESS",
      `A Base re-verification is already in progress in workspace ${running.workspaceName ?? running.workspaceId}.`,
    );
  }
  const lastCompleted = [...mockReverificationRuns]
    .sort((a, b) => b.startedAt.localeCompare(a.startedAt))
    .find((run) => run.state === "completed");
  if (kind === "incremental" && !lastCompleted) {
    return reverificationRefusal(
      422,
      "REVERIFICATION_NOT_COMPLETED",
      "An incremental re-verification needs a completed run; start a full one first.",
    );
  }
  const taken = new Set(mockWorkspaceList().map((w) => w.name));
  let name = input.name;
  for (let suffix = 2; taken.has(name); suffix += 1) {
    name = `${input.name}-${suffix}`;
  }
  const now = new Date().toISOString();
  const id = `ws-${name}`;
  const base = MOCK_WORKSPACES.find((w) => w.id === "base")!;
  const workspace: WorkspaceSummary = {
    ...base,
    id,
    name,
    description: `Base re-verification (${kind})`,
    isBase: false,
    ownerUserId: "user-alex",
    createdBy: "user-alex",
    clonedFromId: "base",
    branches: base.branches.map((branch) => ({
      ...branch,
      workspaceId: id,
      branchName: name,
      hasDraft: false,
      dirtyFileCount: 0,
    })),
    tasksCompleted: 0,
    totalTasks: 0,
    runningTime: null,
    cost: 0,
    concurrentTasks: 0,
    lastUpdated: now,
    createdAt: now,
  };
  const run: ReverificationRun = {
    id: `run-${name}`,
    kind,
    state: "in_progress",
    workspaceId: id,
    workspaceName: name,
    workspaceDeleted: false,
    ownerUserId: "user-alex",
    startedBy: "user-alex",
    startedAt: now,
    completedAt: null,
    label: `Base re-verification ${now.slice(0, 10)} (${kind})`,
    baseHeads: MOCK_BASE_HEADS,
    sinceHeads: kind === "incremental" ? lastCompleted!.baseHeads : null,
  };
  mockReverificationWorkspaces = [...mockReverificationWorkspaces, workspace];
  mockReverificationRuns = [...mockReverificationRuns, run];
  return HttpResponse.json(
    {
      data: {
        workspace,
        clonedBranches: workspace.branches.length,
        clonedDrafts: 0,
        carriedSessions: 0,
        issueLink: null,
        startedChat: null,
        reverificationRun: run,
      } satisfies CloneWorkspaceResponse,
    },
    { status: 201 },
  );
}

/**
 * Reset the scoped-config store to a caller-supplied baseline (or empty
 * scopes). Exported so tests exercising the PUT→merge→GET path can seed a
 * deterministic starting point, since the store is module-level and otherwise
 * carries mutations across tests.
 */
export function resetMockScopedConfig(
  seed?: Partial<
    Record<
      MockScope,
      {
        settings?: Record<string, unknown>;
        secretKeys?: string[];
        secretHints?: Record<string, string>;
      }
    >
  >,
): void {
  for (const scope of ["org", "workspace", "user"] as MockScope[]) {
    mockScopedConfig[scope] = {
      settings: structuredClone(seed?.[scope]?.settings ?? {}),
      secretKeys: [...(seed?.[scope]?.secretKeys ?? [])],
      secretHints: structuredClone(seed?.[scope]?.secretHints ?? {}),
    };
  }
}

function buildScopedConfig(scope: MockScope) {
  const effectiveSettings: Record<string, unknown> = {};
  const effectiveSecret = new Set<string>();
  const effectiveSecretHints: Record<string, string> = {};
  // Merge precedence only up to the requested scope, mirroring the real
  // org -> workspace -> user resolution. Always merging all three would let a
  // scoped request leak narrower-scope values and hide precedence regressions.
  const chain: MockScope[] =
    scope === "org"
      ? ["org"]
      : scope === "workspace"
        ? ["org", "workspace"]
        : ["org", "workspace", "user"];
  for (const s of chain) {
    Object.assign(effectiveSettings, mockScopedConfig[s].settings);
    mockScopedConfig[s].secretKeys.forEach((k) => {
      effectiveSecret.add(k);
      // The winning scope's hint follows the winning value (config route D6):
      // a later scope setting the key replaces the hint, including replacing
      // it with absence when the later value is ineligible.
      if (mockScopedConfig[s].secretHints[k] !== undefined) {
        effectiveSecretHints[k] = mockScopedConfig[s].secretHints[k];
      } else {
        delete effectiveSecretHints[k];
      }
    });
  }
  return {
    scope,
    settings: mockScopedConfig[scope].settings,
    secretKeys: mockScopedConfig[scope].secretKeys,
    secretKeyHints: { ...mockScopedConfig[scope].secretHints },
    effectiveSettings,
    effectiveSecretKeys: [...effectiveSecret].sort(),
    effectiveSecretKeyHints: effectiveSecretHints,
    updatedBy: "user-alex",
    updatedAt: "2024-12-15T07:00:00Z",
  };
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

// Recursive deep merge mirroring the server's `merge_json` (config_service
// D6): when a key holds an object on BOTH sides the objects merge key-by-key;
// otherwise the overlay value replaces. A shallow spread would instead replace
// a nested object wholesale, so an MSW-backed test could pass against behavior
// that differs from production.
function mergeJson(
  base: Record<string, unknown>,
  overlay: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...base };
  for (const [key, overlayValue] of Object.entries(overlay)) {
    const baseValue = out[key];
    if (isPlainObject(baseValue) && isPlainObject(overlayValue)) {
      out[key] = mergeJson(baseValue, overlayValue);
    } else {
      out[key] = overlayValue;
    }
  }
  return out;
}

function applyScopedUpdate(
  scope: MockScope,
  body: {
    settings?: Record<string, unknown>;
    removeSettingKeys?: string[];
    setSecrets?: Record<string, string>;
    removeSecretKeys?: string[];
  },
) {
  // Mirror the server's merge-and-explicit-delete contract (config_service
  // D1/D6). Replacing `settings` wholesale here would make a client that still
  // deletes keys by omission look correct against the mock; a shallow spread
  // would hide nested-object merge regressions.
  if (body.settings !== undefined) {
    mockScopedConfig[scope].settings = mergeJson(
      mockScopedConfig[scope].settings,
      body.settings,
    );
  }
  if (body.removeSettingKeys) {
    // Removal is top-level only, matching the server's `remove_setting_keys`.
    for (const k of body.removeSettingKeys) {
      delete mockScopedConfig[scope].settings[k];
    }
  }
  if (body.setSecrets) {
    for (const [k, v] of Object.entries(body.setSecrets)) {
      if (!mockScopedConfig[scope].secretKeys.includes(k)) {
        mockScopedConfig[scope].secretKeys.push(k);
      }
      // Derive the bounded hint exactly as the server does (config route D6):
      // eligible values store a 12-char hint, ineligible values store none.
      const hint = mockSecretDisplayHint(k, v);
      if (hint !== null) {
        mockScopedConfig[scope].secretHints[k] = hint;
      } else {
        delete mockScopedConfig[scope].secretHints[k];
      }
    }
  }
  if (body.removeSecretKeys) {
    const remove = body.removeSecretKeys;
    mockScopedConfig[scope].secretKeys = mockScopedConfig[
      scope
    ].secretKeys.filter((k) => !remove.includes(k));
    for (const k of remove) {
      delete mockScopedConfig[scope].secretHints[k];
    }
  }
}

// ---------------------------------------------------------------------------
// Org credentials — in-memory API keys + worker registration tokens.
// ---------------------------------------------------------------------------

let mockApiKeys: ApiKey[] = [];

// ---------------------------------------------------------------------------
// Handlers Array
// ---------------------------------------------------------------------------

/**
 * The server's `currentOnly=true` on the suggestions list: only rows whose
 * publication is about the source's current commit (every repository its
 * `baseCommitVector` names is still at that `sourceHead`; a vector naming
 * no repository counts as current). Tests hand it the request and what
 * they seeded.
 */
export function reviewSuggestionsResponse(
  request: Request,
  seeded: {
    suggestions: Array<{
      publicationId: string;
      kind?: string;
      status?: string;
    }>;
    publications?: Array<{ id: string; baseCommitVector: unknown }>;
    sourceHeads?: Record<string, string>;
  },
): { suggestions: Array<{ publicationId: string }> } {
  const url = new URL(request.url);
  const kind = url.searchParams.get("kind");
  const status = url.searchParams.get("status");
  const rows = seeded.suggestions.filter(
    (suggestion) =>
      (!kind || suggestion.kind === kind) &&
      (!status || suggestion.status === status),
  );
  if (url.searchParams.get("currentOnly") !== "true") {
    return { suggestions: rows };
  }
  const heads = seeded.sourceHeads ?? {};
  const current = new Set(
    (seeded.publications ?? [])
      .filter((publication) => {
        const vector = publication.baseCommitVector;
        if (!Array.isArray(vector)) return true;
        return vector.every((entry) => {
          const repoId = (entry as { repoId?: unknown }).repoId;
          const head = (entry as { sourceHead?: unknown }).sourceHead;
          if (typeof repoId !== "string" || typeof head !== "string") {
            return true;
          }
          const now = heads[repoId];
          return !now || now.toLowerCase() === head.toLowerCase();
        });
      })
      .map((publication) => publication.id),
  );
  return {
    suggestions: rows.filter(
      (suggestion) =>
        !seeded.publications || current.has(suggestion.publicationId),
    ),
  };
}

export const handlers = [
  http.get("/api/v1/workspaces/:workspaceId/source-control/menu", () =>
    HttpResponse.json({
      data: { generation: 0, fingerprint: "empty", private: false, repos: [] },
    }),
  ),
  http.get("/api/v1/workspaces/:workspaceId/source-control/checkpoint", () =>
    HttpResponse.json({ data: { generation: 0 } }),
  ),
  http.get("/api/v1/workspaces/:id/review-flow", ({ params }) =>
    HttpResponse.json({
      data: {
        workspace: {
          ownerUserId:
            getOrFirst(MOCK_WORKSPACE_DETAILS, String(params.id))
              ?.ownerUserId ?? "user-alex",
          targetWorkspaceId: null,
          phase: "be",
          readOnly: false,
        },
        pr: null,
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
  http.get("/api/v1/workspaces/:id/review-flow/changes", () =>
    HttpResponse.json({
      data: {
        beforeHeads: null,
        afterHeads: [],
        baselineAvailable: false,
        files: [],
        file: null,
      },
    }),
  ),
  http.get("/api/v1/workspaces/:id/review-flow/history", () =>
    HttpResponse.json({ data: { rows: [], nextCursor: null } }),
  ),

  // =========================================================================
  // UI Telemetry ingest (issue #1552) — the global TelemetryBridge listeners
  // may flush during component tests; without this handler MSW's
  // onUnhandledRequest: "error" would fail unrelated tests.
  // =========================================================================
  http.post("/api/v1/telemetry/ui-events", () =>
    HttpResponse.json({ data: { accepted: 0 } }, { status: 202 }),
  ),

  // =========================================================================
  // Org admin settings — Admin page (platform defaults; PUT echoes the body)
  // =========================================================================
  http.get("/api/v1/orgs/:orgId/admin-settings", ({ params }) =>
    HttpResponse.json({ data: mockOrgAdminSettingsFor(String(params.orgId)) }),
  ),
  http.put(
    "/api/v1/orgs/:orgId/admin-settings",
    async ({ params, request }) => {
      const body = (await request.json().catch(() => ({}))) as Partial<
        typeof DEFAULT_MOCK_ORG_ADMIN_SETTINGS
      >;
      const orgId = String(params.orgId);
      const next = { ...mockOrgAdminSettingsFor(orgId), ...body };
      mockOrgAdminSettings.set(orgId, next);
      return HttpResponse.json({ data: next });
    },
  ),

  // =========================================================================
  // Runtime Config — scoped GET/PUT (org / workspace / user)
  // =========================================================================
  http.get("/api/v1/orgs/:orgId/config", () =>
    HttpResponse.json({ data: buildScopedConfig("org") }),
  ),
  http.put("/api/v1/orgs/:orgId/config", async ({ request }) => {
    applyScopedUpdate(
      "org",
      (await request.json().catch(() => ({}))) as Record<string, never>,
    );
    return HttpResponse.json({ data: buildScopedConfig("org") });
  }),
  http.get("/api/v1/workspaces/:workspaceId/config", () =>
    HttpResponse.json({ data: buildScopedConfig("workspace") }),
  ),
  http.put("/api/v1/workspaces/:workspaceId/config", async ({ request }) => {
    applyScopedUpdate(
      "workspace",
      (await request.json().catch(() => ({}))) as Record<string, never>,
    );
    return HttpResponse.json({ data: buildScopedConfig("workspace") });
  }),
  http.get("/api/v1/users/me/config", () =>
    HttpResponse.json({ data: buildScopedConfig("user") }),
  ),
  // External PR checks snapshot (info?checks=true). Default: no provider repos
  // (nothing external to show/gate); tests needing checks override this.
  http.get("/api/v1/workspaces/:workspaceId/pull-requests/info", () =>
    HttpResponse.json({ data: { source: "github", repositories: [] } }),
  ),
  http.put("/api/v1/users/me/config", async ({ request }) => {
    applyScopedUpdate(
      "user",
      (await request.json().catch(() => ({}))) as Record<string, never>,
    );
    return HttpResponse.json({ data: buildScopedConfig("user") });
  }),
  // =========================================================================
  // Org credentials — API keys (settings page) + worker tokens (worker page)
  // =========================================================================
  http.get("/api/v1/orgs/:orgId/api-keys", () =>
    HttpResponse.json({ data: mockApiKeys }),
  ),
  http.post("/api/v1/orgs/:orgId/api-keys", async ({ params, request }) => {
    const body = (await request.json().catch(() => ({}))) as {
      name?: string;
      scopes?: string[];
      expiresInSecs?: number;
    };
    const id = `key-${Date.now()}`;
    const apiKey: ApiKey = {
      id,
      orgId: String(params.orgId),
      userId: "user-alex",
      name: body.name ?? "api-key",
      keyPrefix: "g5k_demo",
      scopes: body.scopes ?? [],
      createdAt: new Date().toISOString(),
      expiresAt: body.expiresInSecs
        ? new Date(Date.now() + body.expiresInSecs * 1000).toISOString()
        : null,
      lastUsedAt: null,
      revokedAt: null,
    };
    mockApiKeys = [apiKey, ...mockApiKeys];
    return HttpResponse.json(
      { data: { apiKey, key: `g5k_${id}_demo-plaintext-shown-once` } },
      { status: 201 },
    );
  }),
  http.delete("/api/v1/orgs/:orgId/api-keys/:keyId", ({ params }) => {
    mockApiKeys = mockApiKeys.map((k) =>
      k.id === params.keyId ? { ...k, revokedAt: new Date().toISOString() } : k,
    );
    return new HttpResponse(null, { status: 204 });
  }),

  // =========================================================================
  // Auth Endpoints
  // =========================================================================

  // Onboarding progress (users g5.user.onboarding): the step never goes
  // down; the merged profile is stored so the next GET returns it.
  http.patch("/api/v1/users/me", async ({ request }) => {
    const body = (await request.json()) as {
      displayName?: string;
      avatarUrl?: string | null;
      onboardingStep?: number;
      onboardingSkipped?: boolean;
    };
    const current = getStoredTestUser() ?? MOCK_CURRENT_USER;
    const stored = Math.max(
      current.onboardingStep ?? -1,
      body.onboardingStep ?? -1,
    );
    const updated = {
      ...current,
      ...(body.displayName !== undefined
        ? { displayName: body.displayName }
        : {}),
      ...(body.avatarUrl !== undefined ? { avatarUrl: body.avatarUrl } : {}),
      onboardingStep: stored < 0 ? null : stored,
      onboardingSkipped:
        body.onboardingSkipped ?? current.onboardingSkipped ?? false,
    };
    try {
      globalThis.localStorage?.setItem(
        "g5_test_current_user",
        JSON.stringify(updated),
      );
    } catch {
      // Storage-less runs still answer.
    }
    return HttpResponse.json({ data: updated });
  }),

  http.get("/api/v1/users/me", () => {
    if (hasDisabledTestAuth()) {
      return HttpResponse.json(
        {
          error: {
            code: "UNAUTHORIZED",
            message: "Authentication required",
            requestId: "req-auth-disabled",
          },
        },
        { status: 401 },
      );
    }

    return HttpResponse.json({
      data: getStoredTestUser() ?? MOCK_CURRENT_USER,
    });
  }),

  http.post("/api/v1/auth/refresh", () => {
    if (hasDisabledTestAuth()) {
      return HttpResponse.json(
        {
          error: {
            code: "UNAUTHORIZED",
            message: "Refresh disabled for this test render",
            requestId: "req-refresh-disabled",
          },
        },
        { status: 401 },
      );
    }

    return HttpResponse.json({ data: { jwt: getStoredTestJwt() ?? MOCK_JWT } });
  }),

  http.post("/api/v1/auth/logout", () => {
    return new HttpResponse(null, { status: 204 });
  }),

  http.post("/api/v1/auth/cli/approve", async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as {
      approve?: boolean;
    };
    return HttpResponse.json({
      data: { status: body.approve === false ? "denied" : "approved" },
    });
  }),

  http.post("/api/v1/auth/ws-ticket", () => {
    return HttpResponse.json({ data: { ticket: MOCK_JWT } });
  }),

  http.get("/api/v1/auth/me/scopes", () => {
    if (hasDisabledTestAuth()) {
      return HttpResponse.json(
        {
          error: {
            code: "UNAUTHORIZED",
            message: "Authentication required",
            requestId: "req-scopes-disabled",
          },
        },
        { status: 401 },
      );
    }

    const storedUser = getStoredTestUser();
    return HttpResponse.json({
      data: storedUser?.scopes ?? [
        "workspaces:read",
        "workspaces:write",
        "tasks:execute",
        "workers:admin",
        "org:admin",
        "sessions:write",
      ],
    });
  }),

  http.get("/api/v1/resolve", ({ request }) => {
    const url = new URL(request.url);
    const orgParam = url.searchParams.get("org");
    const workspaceParam = url.searchParams.get("workspace");

    const org =
      MOCK_ORGS.find(
        (candidate) =>
          candidate.id === orgParam ||
          slugifyOrgName(candidate.name) === orgParam,
      ) ?? MOCK_ORGS[0];

    // Pages link to `/<org>/<name>`; the server matches the name
    // case-insensitively, so "Auth Refactor" resolves as well as its slug.
    const resolvedWorkspace =
      mockWorkspaceList().find((workspace) => {
        return (
          workspace.id === workspaceParam ||
          workspace.name.toLowerCase() === workspaceParam?.toLowerCase() ||
          slugifyWorkspaceName(workspace.name) === workspaceParam
        );
      }) ??
      (workspaceParam === "test-workspace" ? MOCK_WORKSPACES[0] : undefined);

    return HttpResponse.json({
      data: {
        orgId: org.id,
        workspaceId: resolvedWorkspace?.id,
      },
    });
  }),

  // =========================================================================
  // Workspace Endpoints
  // =========================================================================

  http.get("/api/v1/orgs/:orgId/workspaces", () => {
    return HttpResponse.json({ data: mockWorkspaceList() });
  }),

  // Base re-verification (g5.reverification.status, g5.reverification.run).
  http.get("/api/v1/orgs/:orgId/home/base-reverification", () =>
    HttpResponse.json({ data: mockBaseReverificationStatus() }),
  ),
  http.post("/api/v1/workspaces/:id/clone", ({ params, request }) =>
    mockStartBaseReverification(String(params.id), request),
  ),

  // Personal workspace resolution (personal_workspace g5.personal.routes).
  http.get("/api/v1/orgs/:orgId/me", ({ params }) =>
    HttpResponse.json({
      data: {
        workspaceId: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
        orgId: String(params.orgId),
        userId: "user-alex",
        name: MOCK_PERSONAL_WORKSPACE_DETAIL.name,
        handle: "alex",
      },
    }),
  ),
  http.get("/api/v1/orgs/:orgId/users/:handle/workspace", ({ params }) =>
    params.handle === "alex"
      ? HttpResponse.json({
          data: {
            workspaceId: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
            orgId: String(params.orgId),
            userId: "user-alex",
            name: MOCK_PERSONAL_WORKSPACE_DETAIL.name,
            handle: "alex",
          },
        })
      : HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "Not found" } },
          { status: 404 },
        ),
  ),

  http.get("/api/v1/workspaces/:id", ({ params }) => {
    const id = params.id as string;
    const detail =
      mockReverificationWorkspaceDetail(id) ??
      getOrFirst(MOCK_WORKSPACE_DETAILS, id);
    return HttpResponse.json({ data: withMockWorkspacePhase(id, detail) });
  }),

  // No preview runs unless a test says so: the preview trigger polls this
  // while the owner's VM runs, and the dialog while it is open.
  http.get("/api/v1/workspaces/:id/preview", () =>
    HttpResponse.json({ data: null }),
  ),

  http.get("/api/v1/workspaces/:id/vm", ({ params }) => {
    const workspaceId = params.id as string;
    const detail = getOrFirst(MOCK_WORKSPACE_DETAILS, workspaceId);
    return HttpResponse.json({
      data: createMockWorkspaceVm({
        orgId: detail?.orgId ?? "org-g5labs",
        workspaceId,
        ownerUserId: detail?.ownerUserId ?? "user-alex",
        status: "running",
        hostId: "host-test",
        vmId: `vm-${workspaceId}`,
        imageRef: "g5-test:latest",
        moduleCatalogVersion: "test",
        startedAt: "2024-12-16T08:00:00Z",
        lastActivityAt: "2024-12-16T08:00:00Z",
      }),
    });
  }),

  // Review workspace context: no review by default. Tests seed a review via
  // server.use(...) overrides.
  http.get("/api/v1/workspaces/:id/review", () => {
    return HttpResponse.json({
      data: { asSource: null, asReview: null, openSuggestions: 0 },
    });
  }),
  // Review publications and suggestions: none by default.
  http.get("/api/v1/workspaces/:id/review/publications", () => {
    return HttpResponse.json({
      data: {
        publications: [],
        findings: [],
        reviewerDisplayNames: {},
        sourceHeads: {},
      },
    });
  }),
  http.get("/api/v1/workspaces/:id/review/suggestions/counts", () => {
    return HttpResponse.json({ data: { paths: [] } });
  }),
  http.get("/api/v1/workspaces/:id/review/suggestions", () => {
    return HttpResponse.json({ data: { suggestions: [] } });
  }),
  http.post("/api/v1/workspaces/:id/review/preview", () => {
    return HttpResponse.json({
      data: { files: [], skipped: [], baseCommitVector: [] },
    });
  }),

  http.get("/api/v1/workspaces/:id/changes", ({ params }) => {
    return HttpResponse.json({
      data: {
        workspace_id: params.id as string,
        source: "live",
        attached: true,
        members: [],
      },
    });
  }),

  http.get("/api/v1/sessions/:id/inputs", () => {
    return HttpResponse.json({ data: [] });
  }),
  http.get("/api/v1/sessions/:id/codex-auth", () => {
    return HttpResponse.json({ data: null });
  }),

  http.patch("/api/v1/workspaces/:id", async ({ params, request }) => {
    const id = params.id as string;
    const detail = withMockWorkspacePhase(
      id,
      getOrFirst(MOCK_WORKSPACE_DETAILS, id),
    );
    // Workbench phase (g5.workbench.phases.state): store and echo a valid
    // requested phase like the API's phase-update response; tests asserting
    // refusals override this handler with `server.use`.
    const body = (await request.json().catch(() => ({}))) as {
      phase?: unknown;
    };
    const phase =
      body.phase === "be" || body.phase === "se" || body.phase === "ie"
        ? body.phase
        : undefined;
    if (phase) mockWorkspacePhases.set(id, phase);
    return HttpResponse.json({
      data: phase ? { ...detail, phase } : detail,
    });
  }),

  // Current `WorkspaceStats` shape (Workspace Details renders it).
  http.get("/api/v1/workspaces/:id/stats", () => {
    return HttpResponse.json({
      data: {
        totalCostUsd: 45.0,
        tasksCompleted: 15,
        totalTasks: 20,
        concurrentTasks: 3,
        tasksStopped: 1,
        byTask: [],
        branches: [],
      },
    });
  }),

  http.get("/api/v1/workspaces/:id/sync/pending", () => {
    return HttpResponse.json({ data: [] });
  }),

  http.get("/api/v1/workspaces/:id/pull-requests", () => {
    return HttpResponse.json({ data: [] });
  }),

  // Merge with override preview (g5.workspace.merge.admin-override.formal):
  // refused by default, as for a caller who may not override, so no override
  // is offered unless a test seeds a preview.
  http.get(
    "/api/v1/workspaces/:id/pull-requests/:pullRequestId/merge-override",
    () =>
      HttpResponse.json(
        {
          error: {
            code: "FORBIDDEN",
            message: "Merge with override is not available.",
          },
        },
        { status: 403 },
      ),
  ),

  // The landing's PR description read (git g5.git.pr-description): the saved
  // description with no provider publications unless a test seeds them.
  http.get(
    "/api/v1/workspaces/:id/pull-requests/:pullRequestId/description",
    ({ params }) =>
      HttpResponse.json({
        data: {
          pullRequestId: String(params.pullRequestId),
          description: null,
          descriptionSource: null,
          descriptionHtml: null,
          repositories: [],
        },
      }),
  ),

  // Guided-journey marks (workbench g5.workbench.journey.marks): nothing
  // marked by default; a PUT answers with the mark it was given.
  http.get("/api/v1/workspaces/:id/journey", () => {
    return HttpResponse.json({ data: { marks: [] } });
  }),
  http.put(
    "/api/v1/workspaces/:id/journey/steps/:stepId",
    async ({ params, request }) => {
      const body = (await request.json()) as {
        done: boolean;
        skipped?: boolean;
      };
      return HttpResponse.json({
        data: {
          marks: [
            {
              stepId: String(params.stepId),
              done: body.done,
              skipped: body.skipped ?? false,
              markedAt: new Date().toISOString(),
              actorUserId: "user-alex",
            },
          ],
        },
      });
    },
  ),

  http.get("/api/v1/workspaces/:id/pull-requests/targets", ({ params }) => {
    return HttpResponse.json({
      data: params.id === "base" ? [] : [{ id: "base", name: "base" }],
    });
  }),

  // Workbench configuration (workbench g5.workbench.actions): the built-in
  // action set and personas, as a workspace without committed
  // `.g5/workbench/` files receives them.
  http.get("/api/v1/workspaces/:id/workbench", () => {
    return HttpResponse.json({
      data: {
        revision: "mock-head.builtin0000",
        actions: WORKBENCH_ACTIONS,
        personas: {
          be: { source: "builtin", text: "# Business Engineering persona" },
          se: { source: "builtin", text: "# System Engineering persona" },
          ie: { source: "builtin", text: "# Integration Engineering persona" },
        },
        config: { status: "builtin" },
      },
    });
  }),

  // Preview applications per code repository (none committed by default).
  http.get("/api/v1/workspaces/:id/repos/:repoId/preview-applications", () =>
    HttpResponse.json({ data: [] }),
  ),

  http.get("/api/v1/workspaces/:id/branches", ({ params }) => {
    const id = params.id as string;
    const workspace = MOCK_WORKSPACES.find((candidate) => candidate.id === id);
    return HttpResponse.json({
      data: (workspace?.branches ?? []).map((branch) =>
        mockWorkspaceBranch(id, branch),
      ),
    });
  }),

  http.get("/api/v1/workspaces/:id/notifications", ({ params }) => {
    const id = params.id as string;
    const notifications = MOCK_NOTIFICATIONS[id] ?? [];
    return HttpResponse.json({ data: notifications });
  }),

  http.get("/api/v1/workspaces/:workspaceId/tasks", ({ params, request }) => {
    const url = new URL(request.url);
    const limit = Number(url.searchParams.get("limit") ?? 50);
    const tasks = Object.values(MOCK_TASKS)
      .filter((task) => task.workspaceId === params.workspaceId)
      .map((task) => ({
        id: task.id,
        name: task.name,
        status: task.status,
        taskType: task.taskType,
        costUsd: task.costUsd,
        workspaceId: task.workspaceId,
        sessionId: task.sessionId,
        createdAt: task.assignedAt ?? new Date(0).toISOString(),
        updatedAt:
          task.completedAt ?? task.assignedAt ?? new Date(0).toISOString(),
      }));
    return HttpResponse.json({
      data: { tasks: tasks.slice(0, limit), cursor: null, hasMore: false },
    });
  }),

  http.get("/api/v1/tasks/:taskId", ({ params }) => {
    const taskId = params.taskId as string;
    const task = MOCK_TASKS[taskId] ?? firstValue(MOCK_TASKS);
    return HttpResponse.json({ data: task });
  }),

  http.get("/api/v1/tasks/:taskId/timeline", ({ params }) => {
    const taskId = params.taskId as string;
    const timeline = getOrFirst(MOCK_TASK_TIMELINES, taskId) ?? [];
    return HttpResponse.json({
      data: { entries: timeline, cursor: null, hasMore: false },
    });
  }),

  // =========================================================================
  // Chat Endpoints
  // =========================================================================

  http.get("/api/v1/workspaces/:id/sessions", ({ params }) => {
    const id = params.id as string;
    const sessions = getOrFirst(MOCK_CHAT_SESSIONS, id) ?? [];
    return HttpResponse.json({ data: sessions });
  }),

  http.post("/api/v1/workspaces/:id/sessions", ({ params }) => {
    const id = params.id as string;
    const newSession = {
      id: `session-${Date.now()}`,
      workspaceId: id,
      sessionType: "chat",
      taskId: null,
      scopeType: "workspace",
      scopeRef: null,
      model: CLAUDE_CHAT_DEFAULT_MODEL,
      title: "New Session",
      summary: null,
      tags: [],
      status: "active",
      messageCount: 0,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      totalCostUsd: 0,
      forkedFromId: null,
      branch: null,
      linkedCommits: [],
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    return HttpResponse.json({ data: newSession }, { status: 201 });
  }),

  http.get("/api/v1/sessions/:sessionId", ({ params }) => {
    const sessionId = params.sessionId as string;
    // Find session across all workspaces
    const allSessions = Object.values(MOCK_CHAT_SESSIONS).flat();
    const session =
      allSessions.find((s) => s.id === sessionId) ?? allSessions[0];
    return HttpResponse.json({ data: session });
  }),

  http.get("/api/v1/sessions/:sessionId/timeline", () => {
    const entries = firstValue(MOCK_TASK_TIMELINES) ?? [];
    return HttpResponse.json({
      data: {
        entries,
        cursor: null,
        hasMore: false,
      },
    });
  }),

  http.get("/api/v1/sessions/:sessionId/messages", ({ params }) => {
    const sessionId = params.sessionId as string;
    const messages = getOrFirst(MOCK_CHAT_MESSAGES, sessionId) ?? [];
    return HttpResponse.json({ data: messages });
  }),

  http.post("/api/v1/sessions/:sessionId/messages", () => {
    return HttpResponse.json(
      {
        data: {
          id: `msg-${Date.now()}`,
          sessionId: "test-session",
          role: "user",
          content: "Test message",
          artifacts: [],
          citations: [],
          status: "complete",
          messageIndex: 0,
          createdAt: new Date().toISOString(),
        },
      },
      { status: 201 },
    );
  }),

  http.post("/api/v1/sessions/:sessionId/messages/:messageId/stop", () => {
    return HttpResponse.json({
      data: { stopped: true, disposition: "signalling" },
    });
  }),

  http.get("/api/v1/sessions/:sessionId/live-tail", () => {
    return HttpResponse.json({ data: { state: "none", entries: [] } });
  }),

  http.post("/api/v1/sessions/:sessionId/background/end", () => {
    return HttpResponse.json({ data: { delivered: true } });
  }),

  http.post("/api/v1/sessions/:sessionId/fork", () => {
    const allSessions = Object.values(MOCK_CHAT_SESSIONS).flat();
    const forkedSession = {
      ...(allSessions[0] ?? {}),
      id: `forked-session-${Date.now()}`,
      title: "Forked Session",
    };
    return HttpResponse.json({ data: forkedSession }, { status: 201 });
  }),

  // =========================================================================
  // Comment Endpoints
  // =========================================================================

  http.get("/api/v1/workspaces/:wsId/comments", () => {
    // Return first available comments
    const comments = firstValue(MOCK_COMMENTS) ?? [];
    return HttpResponse.json({ data: comments });
  }),

  http.post("/api/v1/workspaces/:wsId/comments", () => {
    return HttpResponse.json(
      {
        data: {
          id: `comment-${Date.now()}`,
          authorId: MOCK_CURRENT_USER.id,
          author: {
            id: MOCK_CURRENT_USER.id,
            displayName: MOCK_CURRENT_USER.displayName,
            avatarUrl: MOCK_CURRENT_USER.avatarUrl,
            initial: "A",
            color: "blue",
          },
          scope: "node",
          nodeId: "node-test",
          filePath: null,
          lineNumber: null,
          commitSha: "abc123",
          commitRangeBase: null,
          commitRangeHead: null,
          content: "New test comment",
          resolved: false,
          resolvedAt: null,
          originBranch: "main",
          replyCount: 0,
          createdAt: new Date().toISOString(),
        },
      },
      { status: 201 },
    );
  }),

  http.post("/api/v1/comments/:commentId/replies", () => {
    return HttpResponse.json(
      {
        data: {
          id: `reply-${Date.now()}`,
          authorId: MOCK_CURRENT_USER.id,
          content: "Test reply",
          createdAt: new Date().toISOString(),
        },
      },
      { status: 201 },
    );
  }),

  http.patch("/api/v1/comments/:commentId", () => {
    return new HttpResponse(null, { status: 204 });
  }),

  // =========================================================================
  // Settings & Spend Endpoints
  // =========================================================================

  http.get("/api/v1/workspaces/:id/settings", ({ params }) => {
    const id = params.id as string;
    const settings = getOrFirst(MOCK_WORKSPACE_SETTINGS, id);
    return HttpResponse.json({ data: settings });
  }),

  http.put("/api/v1/workspaces/:id/settings", ({ params }) => {
    const id = params.id as string;
    const settings = getOrFirst(MOCK_WORKSPACE_SETTINGS, id);
    return HttpResponse.json({ data: settings });
  }),

  http.get("/api/v1/workspaces/:id/settings/spend", ({ params }) => {
    const id = params.id as string;
    const spend = getOrFirst(MOCK_WORKSPACE_SPEND, id);
    return HttpResponse.json({ data: spend });
  }),

  http.get("/api/v1/workspaces/:id/settings/spend/history", ({ params }) => {
    const id = params.id as string;
    const history = getOrFirst(MOCK_SPEND_HISTORY, id);
    return HttpResponse.json({ data: history });
  }),

  // =========================================================================
  // Org Management Endpoints
  // =========================================================================

  http.get("/api/v1/orgs", () => {
    return HttpResponse.json({ data: MOCK_ORGS });
  }),

  // =========================================================================
  // Platform operator console (g5.organization.platform-operator)
  // =========================================================================
  // Stateless defaults; tests that need a refetched list or an error
  // override these with server.use().

  http.get("/api/v1/platform/orgs", () => {
    return HttpResponse.json({
      data: [
        {
          id: "org-acme",
          name: "Acme Corp",
          slug: "acme",
          createdAt: "2026-01-02T00:00:00Z",
          memberCount: 4,
        },
        {
          id: "org-g5labs",
          name: "G5 Labs",
          slug: "g5labs",
          createdAt: "2026-01-01T00:00:00Z",
          memberCount: 12,
        },
      ],
    });
  }),

  http.post("/api/v1/platform/orgs", async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      slug?: string;
      adminEmail: string;
    };
    return HttpResponse.json(
      {
        data: {
          org: {
            id: "org-new",
            name: body.name,
            slug: body.slug ?? slugifyOrgName(body.name),
            createdAt: new Date().toISOString(),
          },
          admin: {
            userId: "user-new-owner",
            email: body.adminEmail.trim().toLowerCase(),
            accountCreated: true,
          },
        },
      },
      { status: 201 },
    );
  }),

  http.get("/api/v1/workspaces/:workspaceId/approvals", ({ params }) => {
    return HttpResponse.json({
      data: withServerAdmission(
        workspaceApprovalsState(params.workspaceId as string),
      ),
    });
  }),

  http.post(
    "/api/v1/workspaces/:workspaceId/approvals/requirements",
    async ({ params, request }) => {
      const workspaceId = params.workspaceId as string;
      const body = (await request.json()) as { definitionId: string };
      const state = workspaceApprovalsState(workspaceId);
      // The route carries no org param; search every fixture org so tests
      // seeding definitions under other org ids resolve too.
      const definition = Object.values(mockOrgApprovalDefinitionStore)
        .flat()
        .find((d) => d.id === body.definitionId);
      if (!definition) {
        return HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "definition not found" } },
          { status: 404 },
        );
      }
      if (!state.requiredInto.some((r) => r.definition.id === definition.id)) {
        state.requiredInto.push({
          definition,
          attachedBy: { userId: "user-alex", displayName: "Alex Chen" },
          attachedAt: new Date().toISOString(),
        });
        state.attachmentHistory.unshift({
          definitionId: definition.id,
          definitionName: definition.name,
          retired: false,
          actor: { userId: "user-alex", displayName: "Alex Chen" },
          createdAt: new Date().toISOString(),
        });
      }
      return HttpResponse.json({ data: withServerAdmission(state) });
    },
  ),

  http.post(
    "/api/v1/workspaces/:workspaceId/approvals/requirements/:definitionId/retire",
    ({ params }) => {
      const workspaceId = params.workspaceId as string;
      const definitionId = params.definitionId as string;
      const state = workspaceApprovalsState(workspaceId);
      const idx = state.requiredInto.findIndex(
        (r) => r.definition.id === definitionId,
      );
      if (idx === -1) {
        return HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "requirement not found" } },
          { status: 404 },
        );
      }
      const [removed] = state.requiredInto.splice(idx, 1);
      state.attachmentHistory.unshift({
        definitionId,
        definitionName: removed.definition.name,
        retired: true,
        actor: { userId: "user-alex", displayName: "Alex Chen" },
        createdAt: new Date().toISOString(),
      });
      return HttpResponse.json({ data: withServerAdmission(state) });
    },
  ),

  http.post(
    "/api/v1/workspaces/:workspaceId/approvals/decisions",
    async ({ params, request }) => {
      const workspaceId = params.workspaceId as string;
      const body = (await request.json()) as {
        definitionId: string;
        requestId?: string;
        outcome: "approved" | "rejected" | "deferred";
        comment?: string;
        phase: "be" | "se" | "ie";
      };
      const state = workspaceApprovalsState(workspaceId);
      // Match the exact entry (definition × phase); a missing phase on an
      // entry reads as "ie" like the client helper.
      const requirement = state.outgoing?.requirements.find(
        (r) =>
          r.definition.id === body.definitionId &&
          (r.phase ?? "ie") === body.phase,
      );
      if (!requirement) {
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "not an active requirement",
            },
          },
          { status: 422 },
        );
      }
      // The real endpoint records the decision for the authenticated caller
      // (the fixtures authenticate as user-alex), not the first pending row.
      const approver = requirement.approvers.find(
        (a) => a.userId === "user-alex",
      );
      if (approver?.requestId && body.requestId !== approver.requestId) {
        return HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "decision must address the caller's pending request",
            },
          },
          { status: 409 },
        );
      }
      if (approver) {
        // A deferred row is the durable Undo: state derives back to pending.
        if (body.outcome === "deferred") {
          approver.state = "pending";
          approver.comment = null;
          approver.decidedAt = null;
        } else {
          approver.state = body.outcome;
          approver.comment = body.comment ?? null;
          approver.decidedAt = new Date().toISOString();
        }
      }
      requirement.decisionLog.unshift({
        actorType: "human",
        outcome: body.outcome,
        comment: body.comment ?? null,
        actor: {
          userId: approver?.userId ?? "user-alex",
          displayName: approver?.displayName ?? "Alex Chen",
        },
        definitionVersion: requirement.definition.latestVersion,
        superseded: false,
        reviewedGitHeads: [],
        createdAt: new Date().toISOString(),
      });
      // Mirror the real eligibility rule: only a listed, still-member
      // approver can decide again after a withdrawal.
      requirement.callerCanDecide =
        body.outcome === "deferred" && !!approver && approver.stillMember;
      // The fixture roster is the server's projected eligible human roster.
      // Missing policy fields represent legacy all-reviewer versions.
      const humans = requirement.approvers;
      const humanYes = humans.some((a) => a.state === "approved");
      const allHumansYes =
        requirement.definition.humans.length === 0 ||
        (humans.length > 0 && humans.every((a) => a.state === "approved"));
      const agent = requirement.decisionLog.find(
        (d) => d.actorType === "agent" && !d.superseded,
      );
      const agentRequired = !!requirement.definition.agentType;
      const agentYes = agentRequired && agent?.outcome === "approved";
      const all = requirement.definition.requireAllReviewers ?? true;
      const override =
        !all &&
        humanYes &&
        (requirement.definition.humanApprovalsOverrideAgent ?? true);
      const rejected =
        humans.some((a) => a.state === "rejected") ||
        (agentRequired && agent?.outcome === "rejected" && !override);
      const approved =
        (humanYes || agentYes) &&
        (!all || (allHumansYes && (!agentRequired || agentYes)));
      requirement.state = rejected
        ? "rejected"
        : approved
          ? "approved"
          : "pending";
      return HttpResponse.json({ data: withServerAdmission(state) });
    },
  ),

  http.get("/api/v1/orgs/:orgId/approval-definitions", ({ params }) => {
    const orgId = params.orgId as string;
    return HttpResponse.json({
      data: mockOrgApprovalDefinitionStore[orgId] ?? [],
    });
  }),

  http.post(
    "/api/v1/orgs/:orgId/approval-definitions",
    async ({ params, request }) => {
      const orgId = params.orgId as string;
      const body = (await request.json()) as {
        name: string;
        description?: string;
        humanUserIds?: string[];
        requireAllReviewers?: boolean;
        humanApprovalsOverrideAgent?: boolean;
        allowSelfApproval?: boolean;
        agentType?: "simple";
        agentPrompt?: string;
        phases?: ("be" | "se" | "ie")[];
      };
      mockOrgApprovalDefinitionSeq += 1;
      const now = new Date().toISOString();
      const definition: import("../../lib/types").OrgApprovalDefinitionDTO = {
        id: `apprdef-${mockOrgApprovalDefinitionSeq}`,
        latestVersion: 1,
        name: body.name,
        description: body.description ?? null,
        humans: approvalHumansFromIds(orgId, body.humanUserIds ?? []),
        agentType: body.agentType ?? null,
        agentPrompt: body.agentPrompt ?? null,
        invalidatesPrevious: false,
        requireAllReviewers: body.requireAllReviewers ?? false,
        allowSelfApproval: body.allowSelfApproval ?? false,
        humanApprovalsOverrideAgent: body.humanApprovalsOverrideAgent ?? true,
        // Phase binding (g5.workbench.gates): omitted resolves to the IE
        // default, mirroring the server.
        phases: body.phases ?? ["ie"],
        createdBy: { userId: "user-alex", displayName: "Alex Chen" },
        createdAt: now,
        firstCreatedBy: { userId: "user-alex", displayName: "Alex Chen" },
        firstCreatedAt: now,
      };
      (mockOrgApprovalDefinitionStore[orgId] ??= []).push(definition);
      return HttpResponse.json({ data: definition }, { status: 201 });
    },
  ),

  http.post(
    "/api/v1/orgs/:orgId/approval-definitions/:definitionId/versions",
    async ({ params, request }) => {
      const orgId = params.orgId as string;
      const definitionId = params.definitionId as string;
      const body = (await request.json()) as {
        name: string;
        description?: string;
        humanUserIds?: string[];
        requireAllReviewers?: boolean;
        humanApprovalsOverrideAgent?: boolean;
        allowSelfApproval?: boolean;
        agentType?: "simple";
        agentPrompt?: string;
        invalidatesPrevious?: boolean;
        phases?: ("be" | "se" | "ie")[];
      };
      const definition = (mockOrgApprovalDefinitionStore[orgId] ?? []).find(
        (d) => d.id === definitionId,
      );
      if (!definition) {
        return HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "definition not found" } },
          { status: 404 },
        );
      }
      definition.latestVersion += 1;
      definition.name = body.name;
      definition.description = body.description ?? null;
      definition.humans = approvalHumansFromIds(orgId, body.humanUserIds ?? []);
      definition.agentType = body.agentType ?? null;
      definition.agentPrompt = body.agentPrompt ?? null;
      definition.invalidatesPrevious = body.invalidatesPrevious ?? false;
      definition.requireAllReviewers = body.requireAllReviewers ?? false;
      definition.allowSelfApproval = body.allowSelfApproval ?? false;
      definition.humanApprovalsOverrideAgent =
        body.humanApprovalsOverrideAgent ?? true;
      // Phase binding (g5.workbench.gates) — echoed on the latest version.
      definition.phases = body.phases ?? ["ie"];
      definition.createdAt = new Date().toISOString();
      return HttpResponse.json({ data: definition });
    },
  ),

  http.get("/api/v1/orgs/:orgId/members", ({ params }) => {
    const orgId = params.orgId as string;
    const members = getOrFirst(mockOrgMembers, orgId) ?? [];
    return HttpResponse.json({ data: members });
  }),

  ...(["deactivate", "reactivate"] as const).map((action) =>
    http.post(
      `/api/v1/orgs/:orgId/members/:userId/${action}`,
      async ({ params, request }) => {
        const member = mockOrgMembers[params.orgId as string]?.find(
          (m) => m.userId === params.userId,
        );
        if (!member) return new HttpResponse(null, { status: 404 });
        if (member.userId === MOCK_CURRENT_USER.id) {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "Cannot change this member's activity",
              },
            },
            { status: 409 },
          );
        }
        const body = (await request.json()) as { activityRevision: number };
        const active = action === "reactivate";
        if (active !== (member.deactivatedAt === null)) {
          if (body.activityRevision !== member.activityRevision) {
            return HttpResponse.json(
              {
                error: {
                  code: "CONFLICT",
                  message: "Membership changed; refresh before trying again",
                },
              },
              { status: 409 },
            );
          }
          member.deactivatedAt = active ? null : new Date().toISOString();
          member.activityRevision += 1;
          // The local demo has no running compute to shut down.
          member.shutdownPending = false;
        }
        return HttpResponse.json({ data: member });
      },
    ),
  ),
  http.get("/api/v1/orgs/:orgId/members/:userId/shutdown", () =>
    HttpResponse.json({ data: [] }),
  ),
  // Stop by user: the local demo has no running compute, so the hold settles
  // at once and nothing is left pending.
  http.post("/api/v1/orgs/:orgId/members/:userId/stop-workspaces", () =>
    HttpResponse.json(
      { data: { holdId: "mock-hold", joined: false, workspaces: [] } },
      { status: 201 },
    ),
  ),

  http.post("/api/v1/orgs/:orgId/members", () => {
    return HttpResponse.json(
      {
        data: {
          invitation: {
            id: `inv-${Date.now()}`,
            orgId: "org-g5labs",
            email: "newuser@example.com",
            role: "member",
            invitedBy: {
              id: MOCK_CURRENT_USER.id,
              displayName: MOCK_CURRENT_USER.displayName,
              avatarUrl: MOCK_CURRENT_USER.avatarUrl,
              initial: "A",
              color: "blue",
            },
            status: "pending",
            expiresAt: new Date(
              Date.now() + 7 * 24 * 60 * 60 * 1000,
            ).toISOString(),
            createdAt: new Date().toISOString(),
          },
        },
      },
      { status: 201 },
    );
  }),

  http.patch("/api/v1/orgs/:orgId/members/:userId", () => {
    return new HttpResponse(null, { status: 204 });
  }),

  http.delete("/api/v1/orgs/:orgId/members/:userId", () => {
    return new HttpResponse(null, { status: 204 });
  }),

  http.get("/api/v1/orgs/:orgId/invitations", ({ params }) => {
    const orgId = params.orgId as string;
    const invitations = getOrFirst(MOCK_ORG_INVITATIONS, orgId) ?? [];
    return HttpResponse.json({ data: invitations });
  }),

  http.post("/api/v1/orgs/:orgId/invitations/:invId/resend", () => {
    return new HttpResponse(null, { status: 204 });
  }),

  http.delete("/api/v1/orgs/:orgId/invitations/:invId", () => {
    return new HttpResponse(null, { status: 204 });
  }),

  // =========================================================================
  // Worker-Host Endpoints (ORG-scoped cloud VM hosts)
  // =========================================================================

  http.get("/api/v1/orgs/:orgId/worker-hosts/workspaces", () =>
    HttpResponse.json({ data: [] }),
  ),

  http.get("/api/v1/orgs/:orgId/worker-hosts", ({ params, request }) => {
    const orgId = params.orgId as string;
    const url = new URL(request.url);
    const statusFilter = url.searchParams.get("status");

    // Advance the async lifecycle: each GET poll ticks this org's provisioning
    // rows; once a row has been polled enough times the mock reconciler flips
    // it to `ready` and stamps a heartbeat (matching the real heartbeat-gated
    // provisioning→ready transition).
    for (const row of mockWorkerHostStore) {
      if (row.orgId !== orgId || row.status !== "provisioning") continue;
      row.pollsSinceInsert += 1;
      if (row.pollsSinceInsert >= MOCK_WORKER_HOST_READY_AFTER_POLLS) {
        row.status = "ready";
        row.lastHeartbeat = new Date().toISOString();
      }
    }

    let rows = mockWorkerHostStore.filter((r) => r.orgId === orgId);
    if (url.searchParams.get("activeOnly") === "true") {
      rows = rows.filter((r) => r.status !== "terminated");
    }
    if (statusFilter) {
      rows = rows.filter((r) => r.status === statusFilter);
    }
    const orgDefaultBaseImageRef = mockOrgDefaultBaseImageRefs.get(orgId);
    const orgDefaultBaseImageTrack = mockOrgDefaultImageTracks.get(orgId);
    return HttpResponse.json({
      data: {
        hosts: rows.map(workerHostToWire),
        total: rows.length,
        ...(orgDefaultBaseImageRef ? { orgDefaultBaseImageRef } : {}),
        ...(orgDefaultBaseImageTrack ? { orgDefaultBaseImageTrack } : {}),
        promotedImageTracks: MOCK_PROMOTED_IMAGE_TRACKS,
        bootDiskSizing: MOCK_BOOT_DISK_SIZING,
      },
    });
  }),

  http.put(
    "/api/v1/orgs/:orgId/worker-hosts/image-policy",
    async ({ params, request }) => {
      const orgId = params.orgId as string;
      const body = (await request.json().catch(() => ({}))) as {
        imageTrack?: string;
        baseImageRef?: string;
      };
      const track = body.imageTrack?.trim();
      const pin = body.baseImageRef?.trim();
      if (!!track === !!pin) {
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "choose exactly one worker image source",
              requestId: "req-wh-image-policy-422",
            },
          },
          { status: 422 },
        );
      }
      if (track) {
        const selected = MOCK_PROMOTED_IMAGE_TRACKS.find(
          (entry) => entry.name === track,
        );
        if (!selected) {
          return HttpResponse.json(
            {
              error: {
                code: "VALIDATION_ERROR",
                message: "unknown promoted image track",
                requestId: "req-wh-image-policy-track-422",
              },
            },
            { status: 422 },
          );
        }
        mockOrgDefaultImageTracks.set(orgId, track);
        mockOrgDefaultBaseImageRefs.set(orgId, selected.imageRef);
        return HttpResponse.json({
          data: {
            baseImageSourceKind: "track",
            baseImageSourceValue: track,
            baseImageRef: selected.imageRef,
            baseImageDigest: selected.digest,
          },
        });
      }
      if (!pin || !isDigestPinnedBaseImageRef(pin)) {
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "baseImageRef must be digest-pinned",
              requestId: "req-wh-image-policy-pin-422",
            },
          },
          { status: 422 },
        );
      }
      mockOrgDefaultImageTracks.delete(orgId);
      mockOrgDefaultBaseImageRefs.set(orgId, pin);
      return HttpResponse.json({
        data: {
          baseImageSourceKind: "pin",
          baseImageSourceValue: pin,
          baseImageRef: pin,
          baseImageDigest: deriveBaseImageDigest(pin),
        },
      });
    },
  ),

  http.post("/api/v1/orgs/:orgId/worker-hosts", async ({ params, request }) => {
    const orgId = params.orgId as string;
    const body = (await request.json().catch(() => ({}))) as {
      region?: string;
      instanceType?: string;
      imageTrack?: string;
      baseImageRef?: string;
      workerProfile?: string;
      workspaceVmLimit?: number;
      bootDiskGb?: number;
    };
    const region = (body.region ?? "").trim();
    if (!region) {
      return HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "region is required",
            details: { region: "required" },
            requestId: "req-wh-422",
          },
        },
        { status: 422 },
      );
    }
    const instanceType =
      body.instanceType && body.instanceType.trim()
        ? body.instanceType.trim()
        : "c3-standard-4";
    const requestedTrack = body.imageTrack?.trim();
    const requestedPin = body.baseImageRef?.trim();
    if (requestedTrack && requestedPin) {
      return HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "choose either imageTrack or baseImageRef",
            requestId: "req-wh-image-source-conflict-422",
          },
        },
        { status: 422 },
      );
    }
    const policyTrack = requestedTrack ?? mockOrgDefaultImageTracks.get(orgId);
    const policyPin = requestedPin ?? mockOrgDefaultBaseImageRefs.get(orgId);
    const selectedTrack = policyTrack
      ? MOCK_PROMOTED_IMAGE_TRACKS.find((entry) => entry.name === policyTrack)
      : undefined;
    if (policyTrack && !selectedTrack) {
      return HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "unknown promoted image track",
            requestId: "req-wh-image-track-422",
          },
        },
        { status: 422 },
      );
    }
    const baseImageRef = selectedTrack?.imageRef ?? policyPin;
    const baseImageSourceKind = selectedTrack
      ? "track"
      : policyPin
        ? "pin"
        : undefined;
    const baseImageSourceValue = selectedTrack?.name ?? policyPin;
    if (!baseImageRef || !baseImageSourceKind || !baseImageSourceValue) {
      return HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "worker image source is required",
            requestId: "req-wh-image-source-required-422",
          },
        },
        { status: 422 },
      );
    }
    if (!isDigestPinnedBaseImageRef(baseImageRef)) {
      return HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "baseImageRef must be digest-pinned",
            details: {
              field: "baseImageRef",
              error: "base_image_ref must be digest-pinned",
              value: baseImageRef,
            },
            requestId: "req-wh-base-image-422",
          },
        },
        { status: 422 },
      );
    }
    // bootDiskGb: optional boot-disk override. Mirror the backend bound
    // `minGb..=maxGb` of the published sizing (g5.runtime.placement.host-disk)
    // → 422, so the mock contract matches MOCK_BOOT_DISK_SIZING.
    if (body.bootDiskGb !== undefined) {
      const n = body.bootDiskGb;
      if (
        !Number.isInteger(n) ||
        n < MOCK_BOOT_DISK_SIZING.minGb ||
        n > MOCK_BOOT_DISK_SIZING.maxGb
      ) {
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "bootDiskGb out of range",
              details: {
                field: "bootDiskGb",
                error:
                  "bootDiskGb must be a whole number of GB within the supported range",
                value: n,
                min: MOCK_BOOT_DISK_SIZING.minGb,
                max: MOCK_BOOT_DISK_SIZING.maxGb,
              },
              requestId: "req-wh-boot-disk-422",
            },
          },
          { status: 422 },
        );
      }
    }
    // workspaceVmLimit: optional host VM budget. Mirror the backend bound
    // `1..=vCPUs(instanceType)` → 422 (worker_hosts ontology [D53]).
    if (body.workspaceVmLimit !== undefined) {
      const vcpus = Number.parseInt(instanceType.split("-").pop() ?? "", 10);
      const n = body.workspaceVmLimit;
      const maxVms = Number.isInteger(vcpus) && vcpus > 0 ? vcpus : 4;
      if (!Number.isInteger(n) || n < 1 || n > maxVms) {
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "workspaceVmLimit out of range",
              details: {
                field: "workspaceVmLimit",
                error:
                  "workspaceVmLimit must be between 1 and the instance type's vCPU count",
                value: n,
                min: 1,
                max: maxVms,
                instanceType,
              },
              requestId: "req-wh-workspace-vm-limit-422",
            },
          },
          { status: 422 },
        );
      }
    }
    const baseImageDigest = deriveBaseImageDigest(baseImageRef);
    const workerProfile =
      body.workerProfile && body.workerProfile.trim()
        ? body.workerProfile.trim()
        : DEFAULT_WORKER_PROFILE;
    if (!isWorkerProfile(workerProfile)) {
      return HttpResponse.json(
        {
          error: {
            code: "VALIDATION_ERROR",
            message: "workerProfile is not supported",
            details: {
              field: "workerProfile",
              error: "worker_profile is not in the supported profile allowlist",
              value: workerProfile,
              allowed: WORKER_PROFILES,
            },
            requestId: "req-wh-worker-profile-422",
          },
        },
        { status: 422 },
      );
    }
    mockWorkerHostSeq += 1;
    const hostId = `host-${mockWorkerHostSeq}`;
    const createdAt = new Date().toISOString();
    mockWorkerHostStore.push({
      hostId,
      orgId,
      instanceType,
      region,
      status: "provisioning",
      createdAt,
      pollsSinceInsert: 0,
      baseImageRef,
      baseImageDigest,
      baseImageSourceKind,
      baseImageSourceValue,
      workerProfile,
      // Labels are not resolved at provision time — they populate on the first
      // explicit runtime-labels refresh.
      runtimeLabels: {},
    });
    // `hostJwt` is a one-time host credential — returned here to mirror the
    // real 201 body, but the UI ignores it and never persists it. The
    // runtime/profile fields are echoed on the 201 so the add-flow round-trips.
    return HttpResponse.json(
      {
        data: {
          hostId,
          hostJwt: `mock-host-jwt-${hostId}`,
          status: "provisioning",
          region,
          instanceType,
          createdAt,
          workerProfile,
          runtimeLabels: {},
          baseImageRef,
          baseImageDigest,
          baseImageSourceKind,
          baseImageSourceValue,
        },
      },
      { status: 201 },
    );
  }),

  http.delete("/api/v1/orgs/:orgId/worker-hosts/:hostId", ({ params }) => {
    const orgId = params.orgId as string;
    const hostId = params.hostId as string;
    const row = mockWorkerHostStore.find(
      (r) => r.orgId === orgId && r.hostId === hostId,
    );
    if (!row) {
      return HttpResponse.json(
        {
          error: {
            code: "NOT_FOUND",
            message: `worker host ${hostId} not found`,
            requestId: "req-wh-404",
          },
        },
        { status: 404 },
      );
    }
    // Terminate (never delete) — idempotent: re-DELETE on an already
    // terminated row still matches and returns 204.
    row.status = "terminated";
    row.lastHeartbeat = undefined;
    return new HttpResponse(null, { status: 204 });
  }),

  // Refresh a host's base-image OCI runtime labels (PR2 endpoint, PR3 UI). Returns
  // the UPDATED existing WorkerHost DTO (no new response struct). The mock keys
  // its behaviour off the host's persisted baseImageRef:
  //   - contains `unknown-schema` → 422 unsupported_runtime_schema (DB untouched)
  //   - contains `registry-down`  → 503 SERVICE_UNAVAILABLE (DB untouched)
  //   - blank or contains `no-schema` → SUCCESS, absent-schema state
  //     (runtimeSchema null, runtimeLabels {})
  //   - otherwise → SUCCESS, schema "v1" + a few com.g5labs.runtime.* labels
  // 403 is exercised per-test via server.use.
  http.post(
    "/api/v1/orgs/:orgId/worker-hosts/:hostId/runtime-labels/refresh",
    ({ params }) => {
      const orgId = params.orgId as string;
      const hostId = params.hostId as string;
      const row = mockWorkerHostStore.find(
        (r) => r.orgId === orgId && r.hostId === hostId,
      );
      if (!row) {
        return HttpResponse.json(
          {
            error: {
              code: "NOT_FOUND",
              message: `worker host ${hostId} not found`,
              requestId: "req-wh-refresh-404",
            },
          },
          { status: 404 },
        );
      }

      const ref = row.baseImageRef;

      if (!ref) {
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "host has no base image ref",
              details: {
                error:
                  "host has no base_image_ref to resolve runtime labels from",
                code: "no_base_image_ref",
              },
              requestId: "req-wh-refresh-no-base-image",
            },
          },
          { status: 422 },
        );
      }

      if (ref.includes("unknown-schema")) {
        // Validation error — DB untouched.
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "unsupported runtime schema version",
              details: {
                code: "unsupported_runtime_schema",
                supported: ["v1"],
              },
              requestId: "req-wh-refresh-422",
            },
          },
          { status: 422 },
        );
      }

      if (ref.includes("registry-down")) {
        // Registry/network/auth/timeout/oversize — service unavailable, DB untouched.
        return HttpResponse.json(
          {
            error: {
              code: "SERVICE_UNAVAILABLE",
              message: "could not reach the image registry",
              requestId: "req-wh-refresh-503",
            },
          },
          { status: 503 },
        );
      }

      const refreshedAt = new Date().toISOString();
      if (ref.includes("no-schema")) {
        // Absent-schema SUCCESS state: the image opted out of G5 runtime metadata.
        row.runtimeSchema = null;
        row.runtimeLabels = {};
      } else {
        // SUCCESS: a v1-opted image — populate G5-owned runtime labels (NEVER the
        // reserved org.opencontainers.image.* namespace).
        const resolved = filterRuntimeLabels({
          "com.g5labs.runtime.schema": "v1",
          "com.g5labs.runtime.base_image": "debian-bookworm",
          "com.g5labs.runtime.workload.class": "general",
          "com.g5labs.runtime.kernel.features": "kvm,overlayfs",
          "com.g5labs.runtime.toolchain": "node20,python3.12",
          "org.opencontainers.image.title": "reserved-and-dropped",
        });
        row.runtimeSchema = resolved.runtimeSchema;
        row.runtimeLabels = resolved.runtimeLabels;
      }
      row.runtimeLabelsRefreshedAt = refreshedAt;

      return HttpResponse.json({ data: workerHostToWire(row) });
    },
  ),

  // =========================================================================
  // File Tree Endpoints
  // =========================================================================

  http.get(
    "/api/v1/workspaces/:workspaceId/repos/:repoId/files",
    ({ params }) => {
      const branchId = params.repoId as string;
      const tree = getOrFirst(MOCK_FILE_TREES, branchId) ?? [];
      return HttpResponse.json({ data: tree });
    },
  ),

  // Core Ontology looks for findings files directly
  // (g5.documents.findings.files); by default a workspace has none.
  http.get("/api/v1/workspaces/:workspaceId/files/.g5/TLA_verification/*", () =>
    HttpResponse.json(
      { error: { code: "NOT_FOUND", message: "Not found" } },
      { status: 404 },
    ),
  ),

  http.get(
    "/api/v1/workspaces/:workspaceId/repos/:repoId/files/*",
    ({ params, request }) => {
      const branchId = params.repoId as string;
      const url = new URL(request.url);
      const prefix = `/api/v1/workspaces/${params.workspaceId}/repos/${branchId}/files/`;
      const rawPath = decodeURIComponent(
        url.pathname.startsWith(prefix)
          ? url.pathname.slice(prefix.length)
          : "",
      );
      const content =
        getMockFileContent(branchId, rawPath) ??
        firstValue(MOCK_FILE_CONTENT) ??
        "// No content available";
      return HttpResponse.json({
        data: {
          path: rawPath,
          content,
          size: content.length,
          encoding: "utf-8",
          draftDirty: false,
        },
      });
    },
  ),

  // =========================================================================
  // Document Endpoints
  // =========================================================================

  http.get("/api/v1/workspaces/:wsId/docs", () => {
    return HttpResponse.json({ data: MOCK_DOCS });
  }),

  http.get("/api/v1/workspaces/:wsId/docs/:docId", ({ params }) => {
    const docId = params.docId as string;
    const doc = MOCK_DOCS.find((d) => d.id === docId);
    if (!doc) {
      return HttpResponse.json(
        {
          error: {
            code: "NOT_FOUND",
            message: `Doc ${docId} not found`,
            requestId: "req-0",
          },
        },
        { status: 404 },
      );
    }
    return HttpResponse.json({ data: doc });
  }),

  // =========================================================================
  // Static Guide Assets
  // =========================================================================

  http.get("/user-guide/:fileName", ({ params }) => {
    const fileName = String(params.fileName ?? "");
    if (!fileName.endsWith(".md")) {
      return new HttpResponse(null, { status: 404 });
    }

    const title =
      fileName === "getting-started-latest.md" ||
      fileName === "getting-started.md"
        ? "Getting Started"
        : fileName === "onboarding-build-from-scratch.md"
          ? "Using G5 to Build from Scratch"
          : fileName === "onboarding-PM-PoC-legacy.md"
            ? "Using G5 to Enhance an Existing Library"
            : "Guide";

    return HttpResponse.text(`# ${title}\n\nGuide content.`);
  }),

  // =========================================================================
  // Instance Endpoints
  // =========================================================================

  http.get("/api/v1/instance/info", () => {
    return HttpResponse.json({ data: MOCK_INSTANCE_INFO });
  }),

  http.get("/api/v1/instance/health", () => {
    return HttpResponse.json({ data: MOCK_INSTANCE_HEALTH });
  }),

  http.get("/api/v1/instance/chat/models", () => {
    return HttpResponse.json({ data: MOCK_CHAT_MODELS });
  }),

  // =========================================================================
  // External GitHub issues — empty defaults. The `external_issues` feature
  // is off in MOCK_INSTANCE_INFO; tests that enable it override these with
  // `server.use` as needed.
  // =========================================================================

  http.get("/api/v1/orgs/:orgId/issue-domains", () =>
    HttpResponse.json({ data: { domains: [] } }),
  ),

  http.get("/api/v1/orgs/:orgId/issues/search", () =>
    HttpResponse.json({
      data: { results: [], domainErrors: [], domainCount: 0, truncated: false },
    }),
  ),

  // A G5 issue's page lists its attachments: none by default.
  http.get("/api/v1/orgs/:orgId/issues/g5/:number/attachments", () =>
    HttpResponse.json({ data: [] }),
  ),

  // An empty label catalog: the label picker shows "no labels yet".
  http.get("/api/v1/orgs/:orgId/issue-domains/:domainId/labels", ({ params }) =>
    HttpResponse.json({
      data: {
        domainId: params.domainId,
        displayKey: "",
        labels: [],
        nextPage: null,
      },
    }),
  ),

  http.get("/api/v1/orgs/:orgId/issues/resolve", () =>
    HttpResponse.json(
      {
        error: {
          code: "NOT_FOUND",
          message: "issue not found",
          details: { issueError: "not_found" },
          requestId: "",
        },
      },
      { status: 404 },
    ),
  ),

  // No creation target: Create and quick-add render disabled with the reason.
  http.get("/api/v1/orgs/:orgId/issues/create-target", () =>
    HttpResponse.json({
      data: {
        state: "no_target",
        destination: null,
        githubLogin: null,
        assignableUsers: [],
        error: null,
      },
    }),
  ),

  http.get("/api/v1/orgs/:orgId/issues/github/:repositoryId/:number", () =>
    HttpResponse.json(
      {
        error: {
          code: "NOT_FOUND",
          message: "Issue not found",
          details: { issueError: "not_found" },
          requestId: "",
        },
      },
      { status: 404 },
    ),
  ),

  http.get("/api/v1/workspaces/:workspaceId/issue-links", () =>
    HttpResponse.json({ data: { links: [] } }),
  ),

  // No GitHub connection: Done / Reopen and Create render disabled with
  // the connect reason; tests that connect override this.
  http.get("/api/v1/github-app/user-connection", () =>
    HttpResponse.json({ data: { connected: false, githubLogin: null } }),
  ),

  // Workspace Activity: context only (no primary issue), with the workbench
  // phase and pull-request entries (g5.workbench.landing).
  http.get("/api/v1/workspaces/:workspaceId/activity", () =>
    HttpResponse.json({ data: MOCK_WORKSPACE_ACTIVITY }),
  ),
  // Ongoing work (ui g5.ui.activity-current): nothing running by default.
  http.get("/api/v1/workspaces/:workspaceId/activity/current", () =>
    HttpResponse.json({ data: { entries: [] } }),
  ),

  // =========================================================================
  // Waitlist Endpoints (public, no auth)
  // =========================================================================
];
